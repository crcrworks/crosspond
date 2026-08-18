//! Overlay window chrome. `unsafe` stays in this crate.

use std::ffi::c_void;

use objc2_app_kit::{NSColor, NSFloatingWindowLevel, NSWindow, NSWindowCollectionBehavior};

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
}
