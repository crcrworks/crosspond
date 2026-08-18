/// Platform-neutral hotkey events.
///
/// UI and core must not import a concrete global-hotkey crate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HotkeyEvent {
    ToggleLauncher,
}

/// Implemented by each OS. Constructed on the UI/main thread.
pub trait GlobalHotkeyService: Send {
    fn poll(&self) -> Option<HotkeyEvent>;
}
