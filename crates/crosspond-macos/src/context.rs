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
    if app.bundle_id == CROSSPOND_BUNDLE_ID || app.pid == std::process::id() as i32 {
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

#[cfg(target_os = "macos")]
pub(crate) fn frontmost_app() -> Option<AppContext> {
    use objc2_app_kit::NSWorkspace;

    let workspace = NSWorkspace::sharedWorkspace();
    let app = workspace.frontmostApplication()?;
    let name = app
        .localizedName()
        .map(|name| name.to_string())
        .filter(|name| !name.is_empty())?;
    let bundle_id = app
        .bundleIdentifier()
        .map(|id| id.to_string())
        .unwrap_or_default();
    let pid = app.processIdentifier();
    if pid <= 0 {
        return None;
    }
    Some(AppContext {
        name,
        bundle_id,
        pid,
    })
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
    }
}
