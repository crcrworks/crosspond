//! macOS platform bindings.
//!
//! Hotkeys, Keychain, ambient context, and cua-driver computer use live here.

mod accessibility;
mod apps;
#[cfg(target_os = "macos")]
mod ax;
mod calendar;
mod context;
#[cfg(target_os = "macos")]
mod cua;
#[cfg(target_os = "macos")]
mod finder;
mod hotkey;
mod input;
mod keychain;
mod permissions;
mod screenshot;
#[cfg(target_os = "macos")]
mod tcc;

#[cfg(target_os = "macos")]
mod window;

pub use accessibility::MacOsAccessibility;
pub use apps::MacOsApps;
pub use calendar::MacOsCalendar;
pub use context::{MacOsContextCollector, yield_to_other_app};
pub use hotkey::{HotkeyError, MacOsGlobalHotkey};
pub use input::MacOsInput;
pub use keychain::MacOsKeychainSecretStore;
pub use permissions::{PermissionKind, PermissionSnapshot};
pub use screenshot::MacOsScreenshot;
#[cfg(target_os = "macos")]
pub use window::make_ns_window_transparent;

/// True when this process is the active macOS application.
///
/// Japanese IME candidate windows can steal key from the launcher without
/// deactivating Crosspond. Callers should not treat that as "click away".
pub fn application_is_active() -> bool {
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
    #[cfg(target_os = "macos")]
    {
        use objc2::MainThreadMarker;
        use objc2_app_kit::NSApplication;

        let Some(mtm) = MainThreadMarker::new() else {
            return false;
        };
        NSApplication::sharedApplication(mtm).isActive()
    }
}

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

/// All Whole-Mac agent backends sharing one cua-driver child where applicable.
pub fn macos_agent_backends() -> (
    MacOsAccessibility,
    MacOsScreenshot,
    MacOsApps,
    MacOsInput,
    MacOsCalendar,
) {
    #[cfg(target_os = "macos")]
    {
        let host = cua::CuaHost::new();
        (
            MacOsAccessibility::with_host(host.clone()),
            MacOsScreenshot::with_host(host.clone()),
            MacOsApps::with_host(host.clone()),
            MacOsInput::with_host(host),
            MacOsCalendar,
        )
    }
    #[cfg(not(target_os = "macos"))]
    {
        (
            MacOsAccessibility::new(),
            MacOsScreenshot::new(),
            MacOsApps::new(),
            MacOsInput::new(),
            MacOsCalendar,
        )
    }
}
