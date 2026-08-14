//! macOS platform bindings.
//!
//! Hotkeys, Keychain, and ambient context (frontmost app, selected text,
//! Finder selection) live here. Accessibility *actions* and capture come later.

#[cfg(target_os = "macos")]
mod ax;
mod context;
#[cfg(target_os = "macos")]
mod finder;
mod hotkey;
mod keychain;

pub use context::MacOsContextCollector;
pub use hotkey::{HotkeyError, MacOsGlobalHotkey};
pub use keychain::MacOsKeychainSecretStore;
