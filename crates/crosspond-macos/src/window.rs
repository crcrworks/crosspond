//! Overlay window chrome. `unsafe` stays in this crate.

use std::ffi::c_void;

use objc2_app_kit::{NSColor, NSWindow};

/// Clear the NSWindow background so a frameless WebView can be a command bar.
pub fn make_ns_window_transparent(ns_window: *mut c_void) {
    if ns_window.is_null() {
        return;
    }
    // SAFETY: Tauri's `ns_window()` is an `NSWindow *` on the main thread.
    let window = unsafe { &*ns_window.cast::<NSWindow>() };
    window.setOpaque(false);
    window.setBackgroundColor(Some(&NSColor::clearColor()));
}
