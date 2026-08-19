use crosspond_core::{GlobalHotkeyService, HotkeyEvent, LauncherHotkey};
use thiserror::Error;

#[cfg(target_os = "macos")]
use global_hotkey::hotkey::HotKey;
#[cfg(target_os = "macos")]
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};

#[derive(Debug, Error)]
pub enum HotkeyError {
    #[error("failed to create the global hotkey manager: {0}")]
    Manager(String),
    #[error("failed to register {0}: {1}")]
    Register(String, String),
    #[cfg(not(target_os = "macos"))]
    #[error("global hotkeys are only implemented on macOS")]
    Unsupported,
}

/// Isolated behind [`GlobalHotkeyService`]. The manager must be created on the
/// main thread (the Tauri event loop).
pub struct MacOsGlobalHotkey {
    #[cfg(target_os = "macos")]
    manager: GlobalHotKeyManager,
    #[cfg(target_os = "macos")]
    registered: Option<HotKey>,
}

impl MacOsGlobalHotkey {
    pub fn new() -> Result<Self, HotkeyError> {
        #[cfg(not(target_os = "macos"))]
        {
            Err(HotkeyError::Unsupported)
        }

        #[cfg(target_os = "macos")]
        {
            let manager =
                GlobalHotKeyManager::new().map_err(|err| HotkeyError::Manager(err.to_string()))?;
            Ok(Self {
                manager,
                registered: None,
            })
        }
    }
}

#[cfg(target_os = "macos")]
fn platform_hotkey(spec: &LauncherHotkey) -> Result<HotKey, String> {
    spec.to_spec()
        .parse()
        .map_err(|err| format!("failed to register {}: {err}", spec.to_spec()))
}

impl GlobalHotkeyService for MacOsGlobalHotkey {
    fn poll(&self) -> Option<HotkeyEvent> {
        #[cfg(not(target_os = "macos"))]
        {
            None
        }

        #[cfg(target_os = "macos")]
        {
            let expected = self.registered.map(|hotkey| hotkey.id());
            let mut triggered = false;
            while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
                if expected == Some(event.id) && event.state == HotKeyState::Pressed {
                    triggered = true;
                }
            }
            triggered.then_some(HotkeyEvent::ToggleLauncher)
        }
    }

    fn clear_hotkey(&mut self) -> Result<(), String> {
        #[cfg(not(target_os = "macos"))]
        {
            Ok(())
        }

        #[cfg(target_os = "macos")]
        {
            if let Some(old) = self.registered.take() {
                self.manager
                    .unregister(old)
                    .map_err(|err| format!("failed to unregister shortcut: {err}"))?;
            }
            Ok(())
        }
    }

    fn set_hotkey(&mut self, spec: &LauncherHotkey) -> Result<(), String> {
        #[cfg(not(target_os = "macos"))]
        {
            let _ = spec;
            Err(HotkeyError::Unsupported.to_string())
        }

        #[cfg(target_os = "macos")]
        {
            let hotkey = platform_hotkey(spec)?;
            if self
                .registered
                .is_some_and(|current| current.id() == hotkey.id())
            {
                return Ok(());
            }
            let previous = self.registered;
            if let Some(old) = previous {
                self.manager
                    .unregister(old)
                    .map_err(|err| format!("failed to unregister {}: {err}", spec.to_spec()))?;
                self.registered = None;
            }
            match self.manager.register(hotkey) {
                Ok(()) => {
                    self.registered = Some(hotkey);
                    Ok(())
                }
                Err(err) => {
                    if let Some(old) = previous
                        && self.manager.register(old).is_ok()
                    {
                        self.registered = Some(old);
                    }
                    Err(format!("failed to register {}: {err}", spec.to_spec()))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hotkey_error_display_is_useful() {
        let err = HotkeyError::Register("alt+Space".into(), "already registered".into());
        assert!(err.to_string().contains("alt+Space"));
        assert!(err.to_string().contains("already registered"));
    }
}
