//! Thin Windows integration layer: single-instance guard and auto-start.

use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE};
use windows::Win32::System::Threading::CreateMutexW;
use windows::core::w;

use crate::config::RUN_KEY;

const RUN_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// Named mutex guard: the second copy of the app exits immediately.
pub struct SingleInstance(Option<HANDLE>);

impl SingleInstance {
    pub fn acquire() -> Option<Self> {
        // SAFETY: plain Win32 call with a static name.
        unsafe {
            match CreateMutexW(None, true, w!("Local\\mechanical-click-single-instance")) {
                Ok(handle) => {
                    if GetLastError() == ERROR_ALREADY_EXISTS {
                        let _ = CloseHandle(handle);
                        None
                    } else {
                        Some(Self(Some(handle)))
                    }
                }
                // If the mutex cannot be created we still allow the app to run.
                Err(_) => Some(Self(None)),
            }
        }
    }
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        if let Some(handle) = self.0.take() {
            unsafe {
                let _ = CloseHandle(handle);
            }
        }
    }
}

pub fn autostart_command() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| format!("cannot locate executable: {e}"))?;
    Ok(format!("\"{}\" --background", exe.display()))
}

pub fn set_autostart(enabled: bool) -> Result<(), String> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu
        .create_subkey(RUN_PATH)
        .map_err(|e| format!("cannot open registry: {e}"))?;
    if enabled {
        key.set_value(RUN_KEY, &autostart_command()?)
            .map_err(|e| format!("cannot register start-up: {e}"))?;
    } else {
        match key.delete_value(RUN_KEY) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(format!("cannot remove start-up entry: {err}")),
        }
    }
    Ok(())
}

pub fn autostart_enabled() -> bool {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(RUN_PATH)
        .and_then(|key| key.get_value::<String, _>(RUN_KEY))
        .is_ok()
}
