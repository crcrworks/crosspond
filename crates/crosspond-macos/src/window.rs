//! Overlay window chrome. `unsafe` stays in this crate.

use std::ffi::c_void;

use objc2_app_kit::{NSColor, NSFloatingWindowLevel, NSWindow, NSWindowCollectionBehavior};

/// Matches `--radius` on the launcher card so CSS and the window clip agree.
const CORNER_RADIUS: f64 = 12.0;

/// Frameless command-bar NSWindow: clear background, stays on deactivate
/// (Japanese IME palettes), and does not participate in Mission Control.
pub fn make_ns_window_transparent(ns_window: *mut c_void) {
    if ns_window.is_null() {
        return;
    }
    // SAFETY: Tauri's `ns_window()` is an `NSWindow *` on the main thread.
    let window = unsafe { &*ns_window.cast::<NSWindow>() };
    window.setOpaque(false);
    window.setBackgroundColor(Some(&NSColor::clearColor()));
    window.setHidesOnDeactivate(false);
    window.setLevel(NSFloatingWindowLevel);
    window.setCollectionBehavior(
        NSWindowCollectionBehavior::MoveToActiveSpace
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::IgnoresCycle
            | NSWindowCollectionBehavior::Transient,
    );
    clip_rounded_content_view(window);
}

fn clip_rounded_content_view(window: &NSWindow) {
    let Some(view) = window.contentView() else {
        return;
    };
    view.setWantsLayer(true);
    let Some(layer) = view.layer() else {
        return;
    };
    layer.setCornerRadius(CORNER_RADIUS);
    layer.setMasksToBounds(true);
}
