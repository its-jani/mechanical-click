//! `--smoke-test`: headless boot check for the three real subsystems — the
//! low-level keyboard hook, the cpal audio engine (with every embedded sample
//! decoded) and the tray icon. Prints one line per check, then exits; no
//! window is created. Exit code 0 = all green, 1 = a check failed,
//! 124 = a subsystem blocked during initialization (watchdog).

use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use windows::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};

use crate::audio::AudioEngine;
use crate::audio::decode;
use crate::config::{self, Config};
use crate::hook;
use crate::store::{self, PackSource};
use crate::tray::Tray;

fn say(text: &str) {
    // Stdout can be a null handle on the GUI subsystem path; never panic there.
    let _ = writeln!(std::io::stdout(), "{text}");
    let _ = std::io::stdout().flush();
}

/// Release builds are `windows_subsystem` (no console); borrow the parent
/// terminal so the report is visible when launched from a shell.
fn attach_parent_console() {
    unsafe {
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

/// Kills the run if any init step blocks, so CI never waits forever.
fn start_watchdog(secs: u64) -> Arc<AtomicBool> {
    let done = Arc::new(AtomicBool::new(false));
    let guard = done.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(secs));
        if !guard.load(Ordering::Relaxed) {
            say("smoke: TIMEOUT - a subsystem blocked during initialization");
            std::process::exit(124);
        }
    });
    done
}

pub fn run() -> i32 {
    attach_parent_console();
    let done = start_watchdog(30);
    let cfg = Config::load();
    let mut failures = 0;

    say("mechanical-click smoke test");

    // The engine is built first because the hook needs its command sender,
    // but the audio sink is reported as check 2 for a readable transcript.
    let mut engine = AudioEngine::new(cfg.latency_ms);

    // -- [1/3] keyboard hook ---------------------------------------------
    say("[1/3] keyboard hook (WH_KEYBOARD_LL) ...");
    let hook = hook::spawn(engine.sender());
    match &hook {
        Ok(_) => say("      ok: hook thread bound and pumping"),
        Err(e) => {
            say(&format!("      FAIL: {e}"));
            failures += 1;
        }
    }

    // -- [2/3] audio engine + embedded samples ----------------------------
    say("[2/3] audio engine ...");
    if engine.status.ready() {
        say(&format!(
            "      ok: {} @ {} Hz, {} ch, {:.1} ms buffer",
            engine.status.device,
            engine.status.sample_rate,
            engine.status.channels,
            engine.status.latency_ms()
        ));
    } else {
        say(&format!(
            "      FAIL: {}",
            engine.status.error.as_deref().unwrap_or("output stream not running")
        ));
        failures += 1;
    }

    // The active profile (press + release) through the real load path.
    let profiles = config::builtin_profiles();
    let active = profiles
        .iter()
        .find(|p| p.id == cfg.profile_id)
        .unwrap_or(&profiles[0]);
    if let Err(e) = engine.apply_profile(active) {
        say(&format!(
            "      FAIL: active profile \"{}\": {e}",
            active.name
        ));
        failures += 1;
    }

    // Every bundled store sample must decode into RAM.
    let mut decoded = 0usize;
    let mut broken = Vec::new();
    for pack in store::bundled_catalog() {
        if let PackSource::Bundled { press, release } = &pack.source {
            for (name, bytes) in [("press", press), ("release", release)] {
                match decode::decode(bytes, "wav") {
                    Ok(_) => decoded += 1,
                    Err(e) => broken.push(format!("{} {name}: {e}", pack.id)),
                }
            }
        }
    }
    if broken.is_empty() {
        say(&format!(
            "      ok: {decoded} bundled samples decoded into memory ({} profiles selectable)",
            profiles.len()
        ));
    } else {
        for item in broken {
            say(&format!("      FAIL: {item}"));
        }
        failures += 1;
    }

    // -- [3/3] tray icon ----------------------------------------------------
    say("[3/3] tray icon ...");
    match Tray::new(&cfg, &profiles, || {}) {
        Ok(tray) => {
            drop(tray); // removes the icon again
            say("      ok: tray created, menu built, no blocking");
        }
        Err(e) => {
            say(&format!("      FAIL: {e}"));
            failures += 1;
        }
    }

    // Unhook and stop the stream before the report (hook drop posts WM_QUIT).
    drop(hook);
    drop(engine);

    done.store(true, Ordering::Relaxed);
    if failures == 0 {
        say("SMOKE PASSED");
        0
    } else {
        say(&format!("SMOKE FAILED ({failures} check group(s))"));
        1
    }
}
