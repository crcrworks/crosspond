use crosspond_core::{GlobalHotkeyService, HotkeyEvent};
use thiserror::Error;

#[cfg(target_os = "macos")]
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
#[cfg(target_os = "macos")]
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};

#[derive(Debug, Error)]
pub enum HotkeyError {
    #[error("failed to create the global hotkey manager: {0}")]
    Manager(String),
    #[error("failed to register Option+Space: {0}")]
    Register(String),
    #[cfg(not(target_os = "macos"))]
    #[error("global hotkeys are only implemented on macOS")]
    Unsupported,
}

/// Option + Space, isolated behind [`GlobalHotkeyService`].
///
/// The manager must be created on the main thread (GPUI's thread).
pub struct MacOsGlobalHotkey {
    #[cfg(target_os = "macos")]
    _manager: GlobalHotKeyManager,
    #[cfg(target_os = "macos")]
    hotkey_id: u32,
}

impl MacOsGlobalHotkey {
    pub fn register_default() -> Result<Self, HotkeyError> {
        #[cfg(not(target_os = "macos"))]
        {
            Err(HotkeyError::Unsupported)
        }

        #[cfg(target_os = "macos")]
        {
            let manager =
                GlobalHotKeyManager::new().map_err(|err| HotkeyError::Manager(err.to_string()))?;
            let hotkey = HotKey::new(Some(Modifiers::ALT), Code::Space);
            let hotkey_id = hotkey.id();
            manager
                .register(hotkey)
                .map_err(|err| HotkeyError::Register(err.to_string()))?;
            Ok(Self {
                _manager: manager,
                hotkey_id,
            })
        }
    }
}

impl GlobalHotkeyService for MacOsGlobalHotkey {
    fn poll(&self) -> Option<HotkeyEvent> {
        #[cfg(not(target_os = "macos"))]
        {
            None
        }

        #[cfg(target_os = "macos")]
        {
            let mut triggered = false;
            while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
                if event.id == self.hotkey_id && event.state == HotKeyState::Pressed {
                    triggered = true;
                }
            }
            triggered.then_some(HotkeyEvent::ToggleLauncher)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hotkey_error_display_is_useful() {
        let err = HotkeyError::Register("already registered".into());
        assert!(err.to_string().contains("Option+Space"));
    }
}
