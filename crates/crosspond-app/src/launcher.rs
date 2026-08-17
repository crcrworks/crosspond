use std::sync::Arc;
use std::time::Duration;

use crosspond_core::{AgentEvent, ContextCollector, EventPump, GlobalHotkeyService, HotkeyEvent};
use gpui::{
    App, Bounds, Global, Pixels, Timer, TitlebarOptions, WindowBackgroundAppearance, WindowBounds,
    WindowHandle, WindowKind, WindowOptions, point, px, size,
};

use crate::command_window::CommandWindow;

pub const WINDOW_WIDTH: Pixels = px(640.0);
pub const IDLE_HEIGHT: Pixels = px(72.0);
pub const RESULT_HEIGHT: Pixels = px(560.0);
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
        // GPUI 0.2.2 only sets NSResizableWindowMask when a titlebar is present.
        // Keep it transparent and move traffic lights off-screen so the launcher
        // still looks like a command bar.
        titlebar: Some(TitlebarOptions {
            title: None,
            appears_transparent: true,
            traffic_light_position: Some(point(px(-100.0), px(-100.0))),
        }),
        focus: false,
        show: false,
        kind: WindowKind::PopUp,
        is_movable: true,
        is_resizable: true,
        is_minimizable: false,
        window_background: WindowBackgroundAppearance::Transparent,
        app_id: Some("com.crosspond.app".into()),
        window_min_size: Some(size(px(400.0), IDLE_HEIGHT)),
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
    let (claimed_visible, window) = {
        let launcher = cx.global::<Launcher>();
        (launcher.visible, launcher.window)
    };
    // NSPanel hidesOnDeactivate can order the launcher out while `visible` stays
    // true. Treating that as "already open" made Option+Space call hide() and
    // look like a freeze.
    let window_key = window
        .update(cx, |_, window, _| window.is_window_active())
        .unwrap_or(false);
    if claimed_visible && window_key {
        hide(cx);
    } else {
        if claimed_visible && !window_key {
            eprintln!("crosspond: launcher marked visible but window was not key; showing");
        }
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
    let needs_onboarding = crate::settings::needs_onboarding(cx);
    {
        let launcher = cx.global::<Launcher>();
        already_visible = launcher.visible;
        collector = Arc::clone(&launcher.collector);
        window = launcher.window;
    }
    let in_conversation = window
        .update(cx, |view, _, _| view.in_conversation())
        .unwrap_or(false);
    // Collect before Crosspond becomes frontmost, otherwise "this" is ourselves.
    // Skip on first launch so we do not prompt for Accessibility.
    // Skip when restoring an existing conversation so ambient badges stay from that session.
    let ambient =
        (!already_visible && !needs_onboarding && !in_conversation).then(|| collector.collect());
    cx.global_mut::<Launcher>().visible = true;
    cx.activate(true);
    let _ = window.update(cx, |view, window, cx| {
        if needs_onboarding {
            view.enter_onboarding(window, cx);
        } else if let Some(ambient) = ambient {
            view.set_ambient_context(ambient, window, cx);
        }
        view.sync_size_for_show(window);
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
        // Keep any in-flight task running; the poll loop still delivers events while hidden.
        if !view.in_conversation() {
            window.resize(size(WINDOW_WIDTH, IDLE_HEIGHT));
        }
        cx.notify();
    });
    cx.hide();
}

/// Re-collect ambient context after New clears the session while the launcher stays open.
pub fn recollect_ambient(cx: &mut App) {
    if !cx.has_global::<Launcher>() {
        return;
    }
    if crate::settings::needs_onboarding(cx) {
        return;
    }
    let collector;
    let window;
    let visible;
    {
        let launcher = cx.global::<Launcher>();
        visible = launcher.visible;
        collector = Arc::clone(&launcher.collector);
        window = launcher.window;
    }
    if !visible {
        return;
    }
    let ambient = collector.collect();
    let _ = window.update(cx, |view, window, cx| {
        view.set_ambient_context(ambient, window, cx);
    });
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

    // Handle the hotkey before applying a burst of agent events. Streaming
    // deltas re-layout the transcript on this thread and would otherwise delay
    // Option+Space until the whole batch is drawn.
    if matches!(hotkey, Some(HotkeyEvent::ToggleLauncher)) {
        toggle(cx);
    }

    for event in events {
        let _ = crate::settings::apply_event(&event, cx);
        let _ = window.update(cx, |view, window, cx| {
            view.apply_event(event, window, cx);
        });
    }
}
