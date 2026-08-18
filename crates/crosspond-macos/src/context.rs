#[cfg(target_os = "macos")]
use std::sync::Mutex;
#[cfg(target_os = "macos")]
use std::time::Instant;

use crosspond_core::{AppContext, ContextCapsule, ContextCollector, WindowContext};

pub const CROSSPOND_BUNDLE_ID: &str = "com.crosspond.app";
pub const FINDER_BUNDLE_ID: &str = "com.apple.finder";

/// Reads frontmost app / selection. Call on the main thread before activating Crosspond.
pub struct MacOsContextCollector;

impl ContextCollector for MacOsContextCollector {
    fn collect(&self) -> ContextCapsule {
        collect_capsule()
    }
}

fn collect_capsule() -> ContextCapsule {
    #[cfg(not(target_os = "macos"))]
    {
        ContextCapsule::default()
    }

    #[cfg(target_os = "macos")]
    {
        collect_macos()
    }
}

#[cfg(target_os = "macos")]
fn collect_macos() -> ContextCapsule {
    let started = Instant::now();
    let capsule = collect_macos_inner();
    let elapsed_ms = started.elapsed().as_millis();
    if elapsed_ms >= 100 {
        eprintln!("crosspond: ambient collect took {elapsed_ms}ms");
    }
    capsule
}

#[cfg(target_os = "macos")]
fn collect_macos_inner() -> ContextCapsule {
    let Some(app) = frontmost_app() else {
        return ContextCapsule::default();
    };
    if is_host_app(&app) {
        return ContextCapsule::default();
    }

    let mut capsule = ContextCapsule {
        frontmost_app: Some(app.clone()),
        ..ContextCapsule::default()
    };

    // Do not prompt here: collect runs on the main thread before the launcher is
    // shown, so a TCC dialog while hidden would look like a freeze.
    if crate::ax::is_trusted() {
        capsule.focused_window = crate::ax::focused_window_title(app.pid)
            .map(|title| WindowContext { title: Some(title) });
        capsule.selected_text = crate::ax::selected_text(app.pid);
        if is_browser(&app.bundle_id) {
            capsule.page_url = crate::ax::document_url(app.pid);
        }
    }

    if app.bundle_id.eq_ignore_ascii_case(FINDER_BUNDLE_ID) {
        capsule.selected_files = crate::finder::selected_files();
    }

    capsule
}

pub(crate) fn is_host_app(app: &AppContext) -> bool {
    is_host_identity(app.pid, &app.bundle_id, std::process::id())
}

pub(crate) fn is_host_identity(pid: i32, bundle_id: &str, self_pid: u32) -> bool {
    pid == self_pid as i32 || bundle_id.eq_ignore_ascii_case(CROSSPOND_BUNDLE_ID)
}

fn is_ignored_surface(bundle_id: &str, name: &str) -> bool {
    matches!(
        bundle_id.to_ascii_lowercase().as_str(),
        "com.apple.dock"
            | "com.apple.loginwindow"
            | "com.apple.windowmanager"
            | "com.apple.controlcenter"
            | "com.apple.notificationcenterui"
            | "com.apple.spotlight"
            | "com.apple.screencaptureui"
    ) || matches!(
        name,
        "Dock" | "Window Server" | "loginwindow" | "Control Center" | "Notification Center"
    )
}

/// Fast running-app names for the composer `@app` picker.
///
/// Uses `NSWorkspace` only. Do not call cua-driver here: spawning or RPC on
/// the Tauri main thread freezes the launcher.
pub fn list_running_app_names() -> Vec<String> {
    #[cfg(not(target_os = "macos"))]
    {
        Vec::new()
    }
    #[cfg(target_os = "macos")]
    {
        list_running_app_names_macos()
    }
}

/// Give key back to the app the user was in. Call after hiding the launcher
/// while Settings is closed so the next Option+Space is not "this is us".
pub fn yield_to_other_app() {
    #[cfg(target_os = "macos")]
    {
        yield_to_other_app_macos();
    }
}

#[cfg(target_os = "macos")]
static LAST_OTHER: Mutex<Option<AppContext>> = Mutex::new(None);

#[cfg(target_os = "macos")]
pub(crate) fn frontmost_app() -> Option<AppContext> {
    use objc2_app_kit::NSWorkspace;

    let workspace = NSWorkspace::sharedWorkspace();
    if let Some(app) = take_other(app_from_running(
        workspace.frontmostApplication().as_deref(),
    )) {
        return Some(app);
    }
    if let Some(app) = take_other(app_from_running(
        workspace.menuBarOwningApplication().as_deref(),
    )) {
        return Some(app);
    }
    if let Some(app) = take_other(on_screen_other_app()) {
        return Some(app);
    }
    last_if_alive()
}

#[cfg(target_os = "macos")]
fn yield_to_other_app_macos() {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSApplicationActivationOptions, NSRunningApplication};

    let Some(app) = frontmost_app() else {
        return;
    };
    let Some(running) = NSRunningApplication::runningApplicationWithProcessIdentifier(app.pid)
    else {
        return;
    };
    if running.isTerminated() {
        return;
    }
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let ns_app = NSApplication::sharedApplication(mtm);
    ns_app.yieldActivationToApplication(&running);
    let _ = running.activateWithOptions(NSApplicationActivationOptions::empty());
}

#[cfg(target_os = "macos")]
fn take_other(app: Option<AppContext>) -> Option<AppContext> {
    let app =
        app.filter(|app| !is_host_app(app) && !is_ignored_surface(&app.bundle_id, &app.name))?;
    remember(&app);
    Some(app)
}

#[cfg(target_os = "macos")]
fn remember(app: &AppContext) {
    let Ok(mut slot) = LAST_OTHER.lock() else {
        return;
    };
    *slot = Some(app.clone());
}

#[cfg(target_os = "macos")]
fn last_if_alive() -> Option<AppContext> {
    use objc2_app_kit::NSRunningApplication;

    let app = LAST_OTHER.lock().ok()?.clone()?;
    if is_host_app(&app) || is_ignored_surface(&app.bundle_id, &app.name) {
        return None;
    }
    let running = NSRunningApplication::runningApplicationWithProcessIdentifier(app.pid)?;
    if running.isTerminated() {
        return None;
    }
    Some(app)
}

#[cfg(target_os = "macos")]
fn list_running_app_names_macos() -> Vec<String> {
    use objc2_app_kit::{NSApplicationActivationPolicy, NSWorkspace};

    let workspace = NSWorkspace::sharedWorkspace();
    let apps = workspace.runningApplications();
    let mut names = Vec::new();
    for index in 0..apps.count() {
        let running = apps.objectAtIndex(index);
        if running.isTerminated()
            || running.activationPolicy() != NSApplicationActivationPolicy::Regular
        {
            continue;
        }
        let Some(app) = app_from_running(Some(&running)) else {
            continue;
        };
        if !should_list_running_app(&app) {
            continue;
        }
        if names.iter().any(|existing| existing == &app.name) {
            continue;
        }
        names.push(app.name);
    }
    names.sort_by(|a, b| a.to_ascii_lowercase().cmp(&b.to_ascii_lowercase()));
    names
}

fn should_list_running_app(app: &AppContext) -> bool {
    !is_host_app(app) && !is_ignored_surface(&app.bundle_id, &app.name)
}

#[cfg(target_os = "macos")]
fn app_from_running(app: Option<&objc2_app_kit::NSRunningApplication>) -> Option<AppContext> {
    let app = app?;
    let pid = app.processIdentifier();
    if pid <= 0 {
        return None;
    }
    let name = app
        .localizedName()
        .map(|name| name.to_string())
        .filter(|name| !name.is_empty())?;
    let bundle_id = app
        .bundleIdentifier()
        .map(|id| id.to_string())
        .unwrap_or_default();
    Some(AppContext {
        name,
        bundle_id,
        pid,
    })
}

#[cfg(target_os = "macos")]
fn on_screen_other_app() -> Option<AppContext> {
    use core_foundation::array::{CFArray, CFArrayGetValueAtIndex};
    use core_foundation::base::TCFType;
    use core_foundation::dictionary::CFDictionary;
    use core_foundation::string::CFString;
    use objc2_app_kit::NSRunningApplication;
    use std::os::raw::c_void;

    const ON_SCREEN_ONLY: u32 = 1 << 0;
    const EXCLUDE_DESKTOP: u32 = 1 << 4;

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGWindowListCopyWindowInfo(option: u32, relative_to_window: u32) -> *const c_void;
    }

    let raw = unsafe { CGWindowListCopyWindowInfo(ON_SCREEN_ONLY | EXCLUDE_DESKTOP, 0) };
    if raw.is_null() {
        return None;
    }
    // SAFETY: CGWindowListCopyWindowInfo returns a +1 CFArray of CFDictionaries.
    let windows: CFArray = unsafe { CFArray::wrap_under_create_rule(raw.cast()) };
    let pid_key = CFString::from_static_string("kCGWindowOwnerPID");
    let layer_key = CFString::from_static_string("kCGWindowLayer");
    let self_pid = std::process::id() as i32;

    for index in 0..windows.len() {
        let value = unsafe { CFArrayGetValueAtIndex(windows.as_concrete_TypeRef(), index) };
        if value.is_null() {
            continue;
        }
        // SAFETY: each entry is a CFDictionary borrowed from `windows`.
        let dict: CFDictionary = unsafe { CFDictionary::wrap_under_get_rule(value.cast()) };
        let Some(pid) = cf_dict_i32(&dict, &pid_key) else {
            continue;
        };
        if pid <= 0 || pid == self_pid {
            continue;
        }
        if cf_dict_i32(&dict, &layer_key).unwrap_or(0) != 0 {
            continue;
        }
        let Some(running) = NSRunningApplication::runningApplicationWithProcessIdentifier(pid)
        else {
            continue;
        };
        let Some(app) = app_from_running(Some(&running)) else {
            continue;
        };
        if is_host_app(&app) || is_ignored_surface(&app.bundle_id, &app.name) {
            continue;
        }
        return Some(app);
    }
    None
}

#[cfg(target_os = "macos")]
fn cf_dict_i32(
    dict: &core_foundation::dictionary::CFDictionary,
    key: &core_foundation::string::CFString,
) -> Option<i32> {
    use core_foundation::base::TCFType;
    use core_foundation::dictionary::CFDictionaryGetValue;
    use core_foundation::number::CFNumber;

    let value = unsafe { CFDictionaryGetValue(dict.as_concrete_TypeRef(), key.as_CFTypeRef()) };
    if value.is_null() {
        return None;
    }
    // SAFETY: CGWindow list values for pid/layer are CFNumbers owned by the dict.
    let number = unsafe { CFNumber::wrap_under_get_rule(value.cast()) };
    number.to_i32()
}

fn is_browser(bundle_id: &str) -> bool {
    matches!(
        bundle_id,
        "com.apple.Safari"
            | "com.apple.SafariTechnologyPreview"
            | "com.google.Chrome"
            | "com.google.Chrome.canary"
            | "com.brave.Browser"
            | "company.thebrowser.Browser"
            | "org.mozilla.firefox"
            | "com.microsoft.edgemac"
            | "com.operasoftware.Opera"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_crosspond_bundle() {
        assert_eq!(CROSSPOND_BUNDLE_ID, "com.crosspond.app");
        assert_eq!(FINDER_BUNDLE_ID, "com.apple.finder");
        assert!(is_browser("com.apple.Safari"));
        assert!(!is_browser("com.apple.finder"));
        assert!(is_host_identity(9, "com.crosspond.app", 9));
        assert!(is_host_identity(1, "COM.CROSSPOND.APP", 99));
        assert!(!is_host_identity(42, "com.apple.Safari", 9));
        assert!(is_ignored_surface("com.apple.dock", "Dock"));
        assert!(!is_ignored_surface("com.apple.Safari", "Safari"));
        assert!(should_list_running_app(&AppContext {
            name: "Safari".into(),
            bundle_id: "com.apple.Safari".into(),
            pid: 12,
        }));
        assert!(!should_list_running_app(&AppContext {
            name: "Crosspond".into(),
            bundle_id: CROSSPOND_BUNDLE_ID.into(),
            pid: 9,
        }));
        assert!(!should_list_running_app(&AppContext {
            name: "Dock".into(),
            bundle_id: "com.apple.dock".into(),
            pid: 1,
        }));
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn list_running_app_names_is_empty_off_macos() {
        assert!(list_running_app_names().is_empty());
    }
}
