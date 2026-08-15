use gpui::{
    App, ClickEvent, SharedString, Styled, Window, WindowAppearance, div, prelude::*, px, rgb, svg,
};

pub fn is_dark(window: &Window) -> bool {
    matches!(
        window.appearance(),
        WindowAppearance::Dark | WindowAppearance::VibrantDark
    )
}

/// Button chrome from GPUI 0.2.2 `examples/window.rs`, with dark/light colors.
pub fn button(
    id: &'static str,
    label: impl Into<SharedString>,
    dark: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let bg = if dark { rgb(0x3a3a3c) } else { rgb(0xf7f7f7) };
    let border = if dark { rgb(0x48484a) } else { rgb(0xe0e0e0) };
    let text = if dark { rgb(0xf5f5f7) } else { rgb(0x1d1d1f) };
    div()
        .id(id)
        .flex_none()
        .px_3()
        .py_1()
        .bg(bg)
        .text_color(text)
        .active(|this| this.opacity(0.85))
        .border_1()
        .border_color(border)
        .rounded_sm()
        .cursor_pointer()
        .child(label.into())
        .on_click(on_click)
}

pub fn svg_icon(path: &'static str, color: gpui::Rgba) -> impl IntoElement {
    svg()
        .path(path)
        .size(px(14.0))
        .flex_none()
        .overflow_hidden()
        .text_color(color)
}
