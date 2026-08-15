//! macOS platform bindings.
//!
//! Hotkeys, Keychain, ambient context, and Accessibility computer use live here.

mod accessibility;
#[cfg(target_os = "macos")]
mod ax;
mod context;
#[cfg(target_os = "macos")]
mod finder;
mod hotkey;
mod keychain;

pub use accessibility::MacOsAccessibility;
pub use context::MacOsContextCollector;
pub use hotkey::{HotkeyError, MacOsGlobalHotkey};
pub use keychain::MacOsKeychainSecretStore;
