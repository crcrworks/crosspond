use std::time::Duration;

use crosspond_core::{HotkeyEvent, provider_key_is_set};
use crosspond_macos::{application_is_active, yield_to_other_app};
use serde::Serialize;
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, WebviewWindow};

use crate::state::AppState;

pub const WINDOW_WIDTH: f64 = 640.0;
pub const IDLE_HEIGHT: f64 = 72.0;
pub const RESULT_HEIGHT: f64 = 560.0;
const BADGE_LINE_HEIGHT: f64 = 20.0;
const TOP_MARGIN: f64 = 96.0;

pub fn idle_height(badge_lines: usize) -> f64 {
    IDLE_HEIGHT + BADGE_LINE_HEIGHT * badge_lines as f64
}

#[derive(Clone, Debug, Serialize)]
pub struct LauncherShown {
    pub badges: Vec<String>,
    pub onboarding: bool,
    pub ready: bool,
    pub visible: bool,
}

pub fn launcher_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window("launcher")
}

pub fn settings_is_open(app: &AppHandle) -> bool {
    app.get_webview_window("settings").is_some()
}

pub fn apply_transparency(window: &WebviewWindow) {
    #[cfg(target_os = "macos")]
    if let Ok(ptr) = window.ns_window() {
        crosspond_macos::make_ns_window_transparent(ptr);
    }
}

pub fn position_launcher(window: &WebviewWindow) {
    let Ok(Some(monitor)) = window.primary_monitor() else {
        return;
    };
    let screen = monitor.size();
    let scale = monitor.scale_factor();
    let width = screen.width as f64 / scale;
    let x = ((width - WINDOW_WIDTH) / 2.0).max(0.0);
    let _ = window.set_position(LogicalPosition::new(x, TOP_MARGIN));
}

pub fn resize_launcher(
    window: &WebviewWindow,
    compact: bool,
    badge_lines: usize,
    extra_height: f64,
) {
    let height = if compact {
        idle_height(badge_lines) + extra_height.max(0.0)
    } else {
        RESULT_HEIGHT.max(idle_height(badge_lines) + extra_height)
    };
    let _ = window.set_size(LogicalSize::new(WINDOW_WIDTH, height));
}

/// Compact idle bar hides on click-away, not when IME or another Crosspond
/// window takes key. IME candidate palettes resign key without deactivating
/// the app; hiding then leaves Japanese input stuck in roman-only.
pub(crate) fn should_hide_compact_on_blur(
    window_is_key: bool,
    compact: bool,
    composing: bool,
    app_active: bool,
    extra_windows: bool,
) -> bool {
    !window_is_key && compact && !composing && !app_active && !extra_windows
}

pub fn hide_compact_if_unfocused(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let inner = state.lock_inner();
    if should_hide_compact_on_blur(
        false,
        inner.compact,
        inner.composing,
        application_is_active(),
        settings_is_open(app),
    ) {
        drop(inner);
        hide(app);
    }
}

pub fn toggle(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let window = launcher_window(app);
    let window_key = window
        .as_ref()
        .and_then(|window| window.is_focused().ok())
        .unwrap_or(false);
    let claimed_visible = state.lock_inner().visible;
    if claimed_visible && window_key {
        hide(app);
    } else {
        if claimed_visible && !window_key {
            eprintln!("crosspond: launcher marked visible but window was not key; showing");
        }
        show(app);
    }
}

pub fn show(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let Some(window) = launcher_window(app) else {
        return;
    };

    let already_visible = state.lock_inner().visible;
    let in_conversation = state.lock_inner().in_conversation;
    let needs_onboarding = !provider_key_is_set(&*state.secrets);

    // Collect before Crosspond becomes frontmost, otherwise "this" is ourselves.
    let ambient = if !already_visible && !needs_onboarding && !in_conversation {
        Some(state.collector.collect())
    } else {
        None
    };

    let mut inner = state.lock_inner();
    inner.visible = true;
    if let Some(ambient) = ambient {
        inner.ambient = ambient;
    }
    let badges = inner.ambient.badge_lines();
    let compact = inner.compact && !needs_onboarding;
    drop(inner);

    apply_transparency(&window);
    position_launcher(&window);
    resize_launcher(&window, compact || needs_onboarding, badges.len(), 0.0);
    let _ = window.show();
    let _ = window.set_focus();

    let ready = provider_key_is_set(&*state.secrets);
    let _ = app.emit(
        "launcher-shown",
        LauncherShown {
            badges,
            onboarding: needs_onboarding,
            ready,
            visible: true,
        },
    );
}

pub fn hide(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    {
        let mut inner = state.lock_inner();
        if !inner.visible {
            return;
        }
        inner.visible = false;
    }
    if let Some(window) = launcher_window(app) {
        let _ = window.hide();
    }
    if !settings_is_open(app) && application_is_active() {
        yield_to_other_app();
    }
    let _ = app.emit(
        "launcher-shown",
        LauncherShown {
            badges: Vec::new(),
            onboarding: false,
            ready: true,
            visible: false,
        },
    );
}

pub fn recollect_ambient(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    if !provider_key_is_set(&*state.secrets) {
        return;
    }
    if !state.lock_inner().visible {
        return;
    }
    let ambient = state.collector.collect();
    let mut inner = state.lock_inner();
    inner.ambient = ambient;
    let badges = inner.ambient.badge_lines();
    drop(inner);
    let _ = app.emit(
        "launcher-shown",
        LauncherShown {
            badges,
            onboarding: false,
            ready: true,
            visible: true,
        },
    );
}

pub fn start_hotkey_loop(app: AppHandle) {
    std::thread::Builder::new()
        .name("crosspond-hotkey".into())
        .spawn(move || {
            loop {
                std::thread::sleep(Duration::from_millis(32));
                let Some(state) = app.try_state::<AppState>() else {
                    continue;
                };
                let hotkey = state.lock_hotkey().poll();
                if matches!(hotkey, Some(HotkeyEvent::ToggleLauncher)) {
                    let handle = app.clone();
                    let _ = app.run_on_main_thread(move || {
                        toggle(&handle);
                    });
                }
            }
        })
        .expect("hotkey thread");
}

#[cfg(test)]
mod tests {
    use super::should_hide_compact_on_blur;

    #[test]
    fn compact_bar_hides_when_the_user_leaves_the_app() {
        assert!(should_hide_compact_on_blur(
            false, true, false, false, false
        ));
    }

    #[test]
    fn compact_bar_stays_when_ime_or_settings_take_key() {
        assert!(!should_hide_compact_on_blur(
            false, true, false, true, false
        ));
        assert!(!should_hide_compact_on_blur(
            false, true, true, false, false
        ));
        assert!(!should_hide_compact_on_blur(
            false, true, false, false, true
        ));
        assert!(!should_hide_compact_on_blur(true, true, false, true, false));
        assert!(!should_hide_compact_on_blur(
            false, false, false, false, false
        ));
    }
}
