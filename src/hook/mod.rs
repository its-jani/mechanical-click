//! Global low-level keyboard hook (`WH_KEYBOARD_LL`).
//!
//! The hook lives on its own OS thread with a bare `GetMessageW` pump. Nothing
//! in the callback allocates or locks: repeats are tracked with an atomic bit
//! set, settings are atomics and events go through bounded channels.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::thread::{self, JoinHandle};

use crossbeam_channel::{Receiver, Sender, bounded};
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, HC_ACTION, KBDLLHOOKSTRUCT, LLKHF_INJECTED, MSG,
    PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL, WM_KEYDOWN,
    WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

use crate::audio::AudioCmd;

#[derive(Clone, Copy, Debug)]
pub struct KeyEvent {
    pub vk: u32,
    pub down: bool,
}

static AUDIO_TX: OnceLock<Sender<AudioCmd>> = OnceLock::new();
static UI_TX: OnceLock<Sender<KeyEvent>> = OnceLock::new();
static ENABLED: AtomicBool = AtomicBool::new(true);
static ALLOW_REPEAT: AtomicBool = AtomicBool::new(false);
static RELEASE_SOUND: AtomicBool = AtomicBool::new(true);
static CAPTURE_INJECTED: AtomicBool = AtomicBool::new(false);

/// Mirrors the UI's focused state so the hook only wakes a window somebody can
/// actually see. Set from the UI thread every pass.
static UI_FOCUSED: AtomicBool = AtomicBool::new(false);
/// Wakes the UI thread for the keycap animation. Installed once at start-up.
static UI_WAKE: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

fn wake_ui() {
    if UI_FOCUSED.load(Ordering::Relaxed)
        && let Some(wake) = UI_WAKE.get()
    {
        wake();
    }
}

/// Sends a visual-only event to the UI. Nothing is queued while the window is
/// unfocused: the sound already left from this thread, and a hidden window
/// must not accumulate stale key events.
fn notify_ui(event: KeyEvent) {
    if !UI_FOCUSED.load(Ordering::Relaxed) {
        return;
    }
    if let Some(tx) = UI_TX.get() {
        let _ = tx.try_send(event);
    }
    wake_ui();
}

/// One bit per virtual-key code, indexed by `vk / 32`.
static DOWN: [AtomicU32; 8] = [const { AtomicU32::new(0) }; 8];

fn remember_down(vk: u32) -> bool {
    if vk >= 256 {
        return true;
    }
    let mask = 1u32 << (vk % 32);
    let prev = DOWN[(vk / 32) as usize].fetch_or(mask, Ordering::Relaxed);
    prev & mask == 0
}

fn remember_up(vk: u32) {
    if vk < 256 {
        let mask = 1u32 << (vk % 32);
        DOWN[(vk / 32) as usize].fetch_and(!mask, Ordering::Relaxed);
    }
}

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // SAFETY: Windows guarantees a KBDLLHOOKSTRUCT for WH_KEYBOARD_LL.
        let info = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let vk = info.vkCode;
        let injected = info.flags.contains(LLKHF_INJECTED);
        if !injected || CAPTURE_INJECTED.load(Ordering::Relaxed) {
            let msg = wparam.0 as u32;
            let play = ENABLED.load(Ordering::Relaxed);
            match msg {
                WM_KEYDOWN | WM_SYSKEYDOWN => {
                    let fresh = remember_down(vk);
                    let repeat = !fresh;
                    if !repeat || ALLOW_REPEAT.load(Ordering::Relaxed) {
                        notify_ui(KeyEvent { vk, down: true });
                        if play && let Some(tx) = AUDIO_TX.get() {
                            let _ = tx.try_send(AudioCmd::Play);
                        }
                    }
                }
                WM_KEYUP | WM_SYSKEYUP => {
                    remember_up(vk);
                    notify_ui(KeyEvent { vk, down: false });
                    if play
                        && RELEASE_SOUND.load(Ordering::Relaxed)
                        && let Some(tx) = AUDIO_TX.get()
                    {
                        let _ = tx.try_send(AudioCmd::Release);
                    }
                }
                _ => {}
            }
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

pub struct HookHandle {
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
    pub events: Receiver<KeyEvent>,
}

impl HookHandle {
    /// Turns sound generation on/off without tearing the hook down.
    pub fn set_enabled(&self, enabled: bool) {
        ENABLED.store(enabled, Ordering::Relaxed);
    }

    pub fn set_allow_repeat(&self, allow: bool) {
        ALLOW_REPEAT.store(allow, Ordering::Relaxed);
    }

    pub fn set_release_sound(&self, allow: bool) {
        RELEASE_SOUND.store(allow, Ordering::Relaxed);
    }

    pub fn set_capture_injected(&self, allow: bool) {
        CAPTURE_INJECTED.store(allow, Ordering::Relaxed);
    }

    /// Whether the app window currently has keyboard focus.
    pub fn set_focused(&self, focused: bool) {
        UI_FOCUSED.store(focused, Ordering::Relaxed);
    }

    /// Installs the "a key was pressed, repaint the keycap" callback.
    pub fn set_wake(&self, wake: impl Fn() + Send + Sync + 'static) {
        let _ = UI_WAKE.set(Box::new(wake));
    }
}

impl Drop for HookHandle {
    fn drop(&mut self) {
        if self.thread_id != 0 {
            unsafe {
                let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
            }
        }
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

/// Installs the hook. Fails only if Windows refuses the hook itself.
pub fn spawn(audio: Sender<AudioCmd>) -> Result<HookHandle, String> {
    let _ = AUDIO_TX.set(audio);
    let (ui_tx, ui_rx) = bounded(512);
    let _ = UI_TX.set(ui_tx);
    let (ready_tx, ready_rx) = bounded::<(u32, Option<String>)>(1);

    let thread = thread::Builder::new()
        .name("mc-key-hook".into())
        .spawn(move || {
            let thread_id = unsafe { GetCurrentThreadId() };
            let hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), None, 0) };
            match hook {
                Ok(hook) => {
                    let _ = ready_tx.send((thread_id, None));
                    let mut msg = MSG::default();
                    loop {
                        // Returns 0 on WM_QUIT and -1 on error.
                        let ret = unsafe { GetMessageW(&mut msg, None, 0, 0) };
                        if ret.0 <= 0 {
                            break;
                        }
                    }
                    unsafe {
                        let _ = UnhookWindowsHookEx(hook);
                    }
                }
                Err(err) => {
                    let _ = ready_tx.send((0, Some(err.to_string())));
                }
            }
        })
        .map_err(|e| format!("cannot start hook thread: {e}"))?;

    match ready_rx.recv() {
        Ok((0, Some(err))) => {
            let _ = thread.join();
            Err(format!("cannot install keyboard hook: {err}"))
        }
        Ok((thread_id, _)) => Ok(HookHandle {
            thread_id,
            thread: Some(thread),
            events: ui_rx,
        }),
        Err(_) => Err("hook thread stopped during start-up".into()),
    }
}

/// Human label for the visualiser keycap.
pub fn key_label(vk: u32) -> String {
    match vk {
        0x08 => "BKSP".into(),
        0x09 => "TAB".into(),
        0x0D => "ENTER".into(),
        0x10 | 0xA0 | 0xA1 => "SHIFT".into(),
        0x11 | 0xA2 | 0xA3 => "CTRL".into(),
        0x12 | 0xA4 | 0xA5 => "ALT".into(),
        0x14 => "CAPS".into(),
        0x1B => "ESC".into(),
        0x20 => "SPACE".into(),
        0x21 => "PGUP".into(),
        0x22 => "PGDN".into(),
        0x23 => "END".into(),
        0x24 => "HOME".into(),
        0x25 => "LEFT".into(),
        0x26 => "UP".into(),
        0x27 => "RIGHT".into(),
        0x28 => "DOWN".into(),
        0x2D => "INS".into(),
        0x2E => "DEL".into(),
        0x5B | 0x5C => "WIN".into(),
        0x5D => "MENU".into(),
        0x90 => "NUM".into(),
        0x91 => "SCRL".into(),
        0xBA => ";".into(),
        0xBB => "=".into(),
        0xBC => ",".into(),
        0xBD => "-".into(),
        0xBE => ".".into(),
        0xBF => "/".into(),
        0xC0 => "`".into(),
        0xDB => "[".into(),
        0xDC => "\\".into(),
        0xDD => "]".into(),
        0xDE => "'".into(),
        vk @ 0x70..=0x87 => format!("F{}", vk - 0x6F),
        vk @ 0x60..=0x69 => format!("NUM{}", vk - 0x60),
        vk @ 0x41..=0x5A => char::from_u32(vk).unwrap_or('?').to_string(),
        vk @ 0x30..=0x39 => char::from_u32(vk).unwrap_or('?').to_string(),
        _ => "KEY".into(),
    }
}
