//! macOS platform bindings.
//!
//! Hotkeys, Keychain, ambient context, and cua-driver computer use live here.

mod accessibility;
#[cfg(target_os = "macos")]
mod ax;
mod context;
#[cfg(target_os = "macos")]
mod cua;
#[cfg(target_os = "macos")]
mod finder;
mod hotkey;
mod keychain;
mod screenshot;
#[cfg(target_os = "macos")]
mod tcc;

pub use accessibility::MacOsAccessibility;
pub use context::MacOsContextCollector;
pub use hotkey::{HotkeyError, MacOsGlobalHotkey};
pub use keychain::MacOsKeychainSecretStore;
pub use screenshot::MacOsScreenshot;

/// Accessibility and screenshot backends that share one cua-driver child.
pub fn macos_computer_backends() -> (MacOsAccessibility, MacOsScreenshot) {
    #[cfg(target_os = "macos")]
    {
        let host = cua::CuaHost::new();
        (
            MacOsAccessibility::with_host(host.clone()),
            MacOsScreenshot::with_host(host),
        )
    }
    #[cfg(not(target_os = "macos"))]
    {
        (MacOsAccessibility::new(), MacOsScreenshot::new())
    }
}
