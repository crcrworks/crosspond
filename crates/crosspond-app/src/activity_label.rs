//! Cursor-style tool ticker: a sliding label with a flowing highlight while running.
//!
//! GPUI 0.2.2 has no CSS transform on `div` and no `background-clip: text`, so this
//! paints a dim `ShapedLine`, then paints a cosine sheen as clipped slices of
//! brighter copies so the highlight ramps instead of cutting off.
//! Confirm paint signatures against this crate version
//! (`line.paint(origin, line_height, window, cx)`).

use std::time::{Duration, Instant};

use gpui::{
    App, Bounds, ContentMask, Element, ElementId, GlobalElementId, Hsla, InspectorElementId,
    IntoElement, LayoutId, Overflow, Pixels, Point, ShapedLine, SharedString, Style, TextStyle,
    Window, point, px, relative, size,
};

const SLIDE: Duration = Duration::from_millis(280);
const SHIMMER: Duration = Duration::from_millis(1200);
const SHEEN_LEVELS: usize = 16;
const SHEEN_SLICES: usize = 48;

pub(crate) fn activity_label(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    running: bool,
    color: impl Into<Hsla>,
) -> impl IntoElement {
    ActivityLabel {
        id: id.into(),
        text: single_line(text.into()),
        running,
        color: color.into(),
    }
}

struct ActivityLabel {
    id: ElementId,
    text: SharedString,
    running: bool,
    color: Hsla,
}

struct ElementState {
    displayed: SharedString,
    outgoing: Option<SharedString>,
    slide_started: Option<Instant>,
    shimmer_started: Instant,
}

struct LayoutState {
    incoming: SharedString,
    outgoing: Option<SharedString>,
    slide_t: f32,
    shimmer_t: Option<f32>,
    color: Hsla,
}

struct PaintState {
    incoming_dim: Option<ShapedLine>,
    incoming_levels: Vec<ShapedLine>,
    outgoing_dim: Option<ShapedLine>,
    incoming_y: Pixels,
    outgoing_y: Pixels,
    shimmer_t: Option<f32>,
}

impl IntoElement for ActivityLabel {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for ActivityLabel {
    type RequestLayoutState = LayoutState;
    type PrepaintState = PaintState;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        global_id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        window.with_element_state(global_id.expect("ActivityLabel requires an element id"), {
            |prev, window| {
                let now = Instant::now();
                let mut state = prev.unwrap_or_else(|| ElementState {
                    displayed: self.text.clone(),
                    outgoing: None,
                    slide_started: None,
                    shimmer_started: now,
                });

                if state.displayed != self.text {
                    if !state.displayed.is_empty() {
                        state.outgoing = Some(state.displayed.clone());
                        state.slide_started = Some(now);
                    }
                    state.displayed = self.text.clone();
                }

                let (slide_t, sliding) = match state.slide_started {
                    Some(start) => {
                        let linear =
                            (start.elapsed().as_secs_f32() / SLIDE.as_secs_f32()).clamp(0.0, 1.0);
                        if linear >= 1.0 {
                            state.outgoing = None;
                            state.slide_started = None;
                            (1.0, false)
                        } else {
                            (ease_out_quint(linear), true)
                        }
                    }
                    None => (1.0, false),
                };

                let shimmer_t = if self.running {
                    Some(
                        (state.shimmer_started.elapsed().as_secs_f32() / SHIMMER.as_secs_f32())
                            % 1.0,
                    )
                } else {
                    None
                };

                if sliding || self.running {
                    window.request_animation_frame();
                }

                let mut style = Style::default();
                style.size.width = relative(1.).into();
                style.size.height = window.line_height().into();
                style.min_size.width = px(0.).into();
                style.overflow.x = Overflow::Hidden;
                style.overflow.y = Overflow::Hidden;

                let layout = LayoutState {
                    incoming: state.displayed.clone(),
                    outgoing: state.outgoing.clone(),
                    slide_t,
                    shimmer_t,
                    color: self.color,
                };
                ((window.request_layout(style, [], cx), layout), state)
            }
        })
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
        let style = window.text_style();
        let line_height = window.line_height();
        let (outgoing_y, incoming_y) = slide_offsets(layout.slide_t, line_height);
        let dim = if layout.shimmer_t.is_some() {
            dimmed_color(layout.color)
        } else {
            layout.color
        };
        let bright = highlight_color(layout.color);
        let incoming_levels = if layout.shimmer_t.is_some() {
            (1..=SHEEN_LEVELS)
                .filter_map(|step| {
                    let amount = step as f32 / SHEEN_LEVELS as f32;
                    shape_label(
                        &layout.incoming,
                        &style,
                        lerp_hsla(dim, bright, amount),
                        1.0,
                        window,
                    )
                })
                .collect()
        } else {
            Vec::new()
        };
        PaintState {
            outgoing_dim: layout
                .outgoing
                .as_ref()
                .filter(|_| layout.slide_t < 1.0)
                .and_then(|text| shape_label(text, &style, dim, 1.0 - layout.slide_t, window)),
            incoming_dim: shape_label(&layout.incoming, &style, dim, 1.0, window),
            incoming_levels,
            incoming_y,
            outgoing_y,
            shimmer_t: layout.shimmer_t,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let line_height = window.line_height();
        let incoming_origin = point(bounds.origin.x, bounds.origin.y + prepaint.incoming_y);
        let outgoing_origin = point(bounds.origin.x, bounds.origin.y + prepaint.outgoing_y);
        let shimmer_t = prepaint.shimmer_t;
        let incoming_levels = std::mem::take(&mut prepaint.incoming_levels);
        let incoming_dim = prepaint.incoming_dim.take();
        let outgoing_dim = prepaint.outgoing_dim.take();

        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            if let Some(line) = outgoing_dim {
                let _ = line.paint(outgoing_origin, line_height, window, cx);
            }
            if let Some(line) = incoming_dim {
                let text_width = line.width;
                let _ = line.paint(incoming_origin, line_height, window, cx);
                if let Some(t) = shimmer_t {
                    paint_sheen(
                        incoming_origin,
                        text_width,
                        line_height,
                        t,
                        &incoming_levels,
                        window,
                        cx,
                    );
                }
            }
        });
    }
}

fn paint_sheen(
    origin: Point<Pixels>,
    text_width: Pixels,
    line_height: Pixels,
    t: f32,
    levels: &[ShapedLine],
    window: &mut Window,
    cx: &mut App,
) {
    if levels.is_empty() {
        return;
    }
    let band = sheen_bounds(origin.x, origin.y, text_width, line_height, t);
    let slice_w = band.size.width / SHEEN_SLICES as f32;
    if slice_w <= px(0.) {
        return;
    }
    for i in 0..SHEEN_SLICES {
        let u0 = i as f32 / SHEEN_SLICES as f32;
        let u1 = (i + 1) as f32 / SHEEN_SLICES as f32;
        let intensity = sheen_falloff((u0 + u1) * 0.5);
        if intensity < 0.04 {
            continue;
        }
        let level = ((intensity * levels.len() as f32).ceil() as usize).clamp(1, levels.len()) - 1;
        let slice = Bounds {
            origin: point(band.origin.x + slice_w * i as f32, band.origin.y),
            size: size(slice_w + px(0.6), band.size.height),
        };
        window.with_content_mask(Some(ContentMask { bounds: slice }), |window| {
            let _ = levels[level].paint(origin, line_height, window, cx);
        });
    }
}

fn shape_label(
    text: &SharedString,
    style: &TextStyle,
    color: Hsla,
    fade: f32,
    window: &mut Window,
) -> Option<ShapedLine> {
    if text.is_empty() || fade <= 0.0 {
        return None;
    }
    let font_size = style.font_size.to_pixels(window.rem_size());
    let mut run = style.to_run(text.len());
    run.color = color.opacity(fade);
    Some(
        window
            .text_system()
            .shape_line(text.clone(), font_size, &[run], None),
    )
}

fn single_line(text: SharedString) -> SharedString {
    if text.as_ref().bytes().any(|b| b == b'\n' || b == b'\r') {
        SharedString::from(text.replace(['\n', '\r'], " "))
    } else {
        text
    }
}

fn ease_out_quint(t: f32) -> f32 {
    1.0 - (1.0 - t).clamp(0.0, 1.0).powi(5)
}

fn slide_offsets(t: f32, line_height: Pixels) -> (Pixels, Pixels) {
    let t = t.clamp(0.0, 1.0);
    (line_height * -t, line_height * (1.0 - t))
}

fn sheen_bounds(
    origin_x: Pixels,
    origin_y: Pixels,
    text_width: Pixels,
    line_height: Pixels,
    t: f32,
) -> Bounds<Pixels> {
    let t = t.clamp(0.0, 1.0);
    let band = (text_width * 0.55).max(px(48.));
    let x = origin_x - band + (text_width + band * 2.) * t;
    Bounds {
        origin: point(x, origin_y),
        size: size(band, line_height),
    }
}

fn sheen_falloff(u: f32) -> f32 {
    let d = (2.0 * u.clamp(0.0, 1.0) - 1.0).abs();
    ((1.0 + (d * std::f32::consts::PI).cos()) * 0.5).clamp(0.0, 1.0)
}

fn lerp_hsla(a: Hsla, b: Hsla, t: f32) -> Hsla {
    let t = t.clamp(0.0, 1.0);
    Hsla {
        h: a.h + (b.h - a.h) * t,
        s: a.s + (b.s - a.s) * t,
        l: a.l + (b.l - a.l) * t,
        a: a.a + (b.a - a.a) * t,
    }
}

fn highlight_color(base: Hsla) -> Hsla {
    if base.l >= 0.5 {
        Hsla {
            h: 0.0,
            s: 0.0,
            l: 1.0,
            a: 1.0,
        }
    } else {
        Hsla {
            h: 0.0,
            s: 0.0,
            l: 0.0,
            a: 1.0,
        }
    }
}

fn dimmed_color(base: Hsla) -> Hsla {
    if base.l >= 0.5 {
        Hsla {
            h: base.h,
            s: base.s,
            l: (base.l * 0.72).clamp(0.25, 1.0),
            a: base.a,
        }
    } else {
        Hsla {
            h: base.h,
            s: base.s,
            l: (base.l + (1.0 - base.l) * 0.28).clamp(0.0, 0.7),
            a: base.a,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{hsla, rgb};

    #[test]
    fn slide_starts_below_and_ends_in_place() {
        let h = px(20.);
        let (out0, in0) = slide_offsets(0.0, h);
        assert_eq!(out0, px(0.));
        assert_eq!(in0, h);
        let (out1, in1) = slide_offsets(1.0, h);
        assert_eq!(out1, px(-20.));
        assert_eq!(in1, px(0.));
    }

    #[test]
    fn sheen_falloff_peaks_in_the_center() {
        assert!((sheen_falloff(0.5) - 1.0).abs() < 0.01);
        assert!(sheen_falloff(0.0) < 0.02);
        assert!(sheen_falloff(1.0) < 0.02);
        let edge = sheen_falloff(0.25);
        assert!(edge > 0.45 && edge < 0.55);
    }

    #[test]
    fn sheen_enters_from_the_left_and_leaves_to_the_right() {
        let text_width = px(100.);
        let start = sheen_bounds(px(10.), px(0.), text_width, px(16.), 0.0);
        let end = sheen_bounds(px(10.), px(0.), text_width, px(16.), 1.0);
        assert!(start.right() <= px(10.) + px(0.5));
        assert!(end.origin.x >= px(10.) + text_width - px(0.5));
    }

    #[test]
    fn dark_muted_highlight_is_white() {
        let muted = Hsla::from(rgb(0x8e8e93));
        assert!(muted.l >= 0.5);
        assert_eq!(highlight_color(muted).l, 1.0);
        assert!(dimmed_color(muted).l < muted.l);
    }

    #[test]
    fn light_muted_highlight_is_black() {
        let muted = Hsla::from(rgb(0x6e6e73));
        assert!(muted.l < 0.5);
        assert_eq!(highlight_color(muted).l, 0.0);
    }

    #[test]
    fn highlight_moves_toward_white_on_dark_text_color() {
        let muted = hsla(0.0, 0.0, 0.6, 1.0);
        assert!(highlight_color(muted).l > muted.l);
    }

    #[test]
    fn ease_out_quint_is_fast_then_settles() {
        assert!((ease_out_quint(0.0) - 0.0).abs() < f32::EPSILON);
        assert!((ease_out_quint(1.0) - 1.0).abs() < f32::EPSILON);
        assert!(ease_out_quint(0.4) > 0.4);
    }
}
