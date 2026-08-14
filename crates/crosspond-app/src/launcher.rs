use std::sync::Arc;
use std::time::Duration;

use crosspond_core::{AgentEvent, ContextCollector, EventPump, GlobalHotkeyService, HotkeyEvent};
use gpui::{
    App, Bounds, Global, Pixels, Timer, WindowBackgroundAppearance, WindowBounds, WindowHandle,
    WindowKind, WindowOptions, point, px, size,
};

use crate::command_window::CommandWindow;

pub const WINDOW_WIDTH: Pixels = px(640.0);
pub const IDLE_HEIGHT: Pixels = px(72.0);
pub const RESULT_HEIGHT: Pixels = px(320.0);
const BADGE_LINE_HEIGHT: Pixels = px(20.0);

const POLL_INTERVAL: Duration = Duration::from_millis(32);
const TOP_MARGIN: Pixels = px(96.0);

pub fn idle_height(badge_lines: usize) -> Pixels {
    IDLE_HEIGHT + BADGE_LINE_HEIGHT * badge_lines as f32
}

struct NoopHotkey;

impl GlobalHotkeyService for NoopHotkey {
    fn poll(&self) -> Option<HotkeyEvent> {
        None
    }
}

pub struct Launcher {
    window: WindowHandle<CommandWindow>,
    events: EventPump,
    hotkey: Box<dyn GlobalHotkeyService>,
    collector: Arc<dyn ContextCollector>,
    visible: bool,
}

impl Global for Launcher {}

pub fn mark_hidden(cx: &mut App) {
    if cx.has_global::<Launcher>() {
        cx.global_mut::<Launcher>().visible = false;
    }
}

pub fn window_options(cx: &App) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(launcher_bounds(cx))),
        titlebar: None,
        focus: false,
        show: false,
        kind: WindowKind::PopUp,
        is_movable: true,
        is_resizable: false,
        is_minimizable: false,
        window_background: WindowBackgroundAppearance::Transparent,
        app_id: Some("com.crosspond.app".into()),
        window_min_size: Some(size(px(320.0), IDLE_HEIGHT)),
        ..Default::default()
    }
}

fn launcher_bounds(cx: &App) -> Bounds<Pixels> {
    let display_bounds = cx
        .primary_display()
        .map(|display| display.bounds())
        .unwrap_or_else(|| Bounds {
            origin: point(px(0.0), px(0.0)),
            size: size(px(1440.0), px(900.0)),
        });
    let origin = point(
        display_bounds.origin.x + (display_bounds.size.width - WINDOW_WIDTH) / 2.0,
        display_bounds.origin.y + TOP_MARGIN,
    );
    Bounds {
        origin,
        size: size(WINDOW_WIDTH, IDLE_HEIGHT),
    }
}

pub fn install(
    window: WindowHandle<CommandWindow>,
    events: EventPump,
    hotkey: Box<dyn GlobalHotkeyService>,
    collector: Arc<dyn ContextCollector>,
    cx: &mut App,
) {
    cx.set_global(Launcher {
        window,
        events,
        hotkey,
        collector,
        visible: false,
    });
    start_poll_loop(cx);
}

pub fn noop_hotkey() -> Box<dyn GlobalHotkeyService> {
    Box::new(NoopHotkey)
}

pub fn toggle(cx: &mut App) {
    if !cx.has_global::<Launcher>() {
        return;
    }
    if cx.global::<Launcher>().visible {
        hide(cx);
    } else {
        show(cx);
    }
}

pub fn show(cx: &mut App) {
    if !cx.has_global::<Launcher>() {
        return;
    }
    let already_visible;
    let collector;
    let window;
    {
        let launcher = cx.global::<Launcher>();
        already_visible = launcher.visible;
        collector = Arc::clone(&launcher.collector);
        window = launcher.window;
    }
    // Collect before Crosspond becomes frontmost, otherwise "this" is ourselves.
    let ambient = (!already_visible).then(|| collector.collect());
    cx.global_mut::<Launcher>().visible = true;
    cx.activate(true);
    let _ = window.update(cx, |view, window, cx| {
        if let Some(ambient) = ambient {
            view.set_ambient_context(ambient, window, cx);
        }
        window.activate_window();
        let focus = view.input_focus_handle(cx);
        window.focus(&focus);
        cx.notify();
    });
}

pub fn hide(cx: &mut App) {
    if !cx.has_global::<Launcher>() {
        return;
    }
    let window = {
        let launcher = cx.global_mut::<Launcher>();
        launcher.visible = false;
        launcher.window
    };
    let _ = window.update(cx, |view, window, cx| {
        view.reset_session(cx);
        window.resize(size(WINDOW_WIDTH, IDLE_HEIGHT));
    });
    cx.hide();
}

fn start_poll_loop(cx: &App) {
    cx.spawn(async move |cx| {
        loop {
            Timer::after(POLL_INTERVAL).await;
            if cx.update(poll_once).is_err() {
                break;
            }
        }
    })
    .detach();
}

fn poll_once(cx: &mut App) {
    if !cx.has_global::<Launcher>() {
        return;
    }

    let mut events: Vec<AgentEvent> = Vec::new();
    let hotkey;
    let window;
    {
        let launcher = cx.global_mut::<Launcher>();
        while let Some(event) = launcher.events.try_recv() {
            events.push(event);
        }
        hotkey = launcher.hotkey.poll();
        window = launcher.window;
    }

    for event in events {
        if crate::settings::apply_event(&event, cx) {
            continue;
        }
        let _ = window.update(cx, |view, window, cx| {
            view.apply_event(event, window, cx);
        });
    }

    if matches!(hotkey, Some(HotkeyEvent::ToggleLauncher)) {
        toggle(cx);
    }
}
