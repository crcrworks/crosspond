//! Text input adapted from GPUI 0.2.2 `examples/input.rs`.
//!
//! Settings fields stay single-line. The command window uses [`TextInput::multiline`]:
//! Enter still submits there; Shift+Enter inserts a newline.
//!
//! Do not replace this with later Zed `main` input examples; paint / layout
//! signatures differ.

use std::ops::Range;

use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, FocusHandle, Focusable, GlobalElementId, KeyBinding, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point, ShapedLine,
    SharedString, Style, TextAlign, TextRun, UTF16Selection, UnderlineStyle, Window,
    WindowAppearance, WrappedLine, actions, div, fill, hsla, point, prelude::*, px, relative, rgb,
    rgba, size,
};
use unicode_segmentation::UnicodeSegmentation;

actions!(
    text_input,
    [
        Backspace,
        Delete,
        Left,
        Right,
        SelectLeft,
        SelectRight,
        SelectAll,
        Home,
        End,
        Up,
        Down,
        SelectUp,
        SelectDown,
        Newline,
        ShowCharacterPalette,
        Paste,
        Cut,
        Copy,
    ]
);

const INPUT_LINE_HEIGHT: Pixels = px(28.);
const MAX_VISIBLE_LINES: usize = 8;

pub fn key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("backspace", Backspace, Some("TextInput")),
        KeyBinding::new("delete", Delete, Some("TextInput")),
        KeyBinding::new("left", Left, Some("TextInput")),
        KeyBinding::new("right", Right, Some("TextInput")),
        KeyBinding::new("shift-left", SelectLeft, Some("TextInput")),
        KeyBinding::new("shift-right", SelectRight, Some("TextInput")),
        KeyBinding::new("up", Up, Some("TextInput")),
        KeyBinding::new("down", Down, Some("TextInput")),
        KeyBinding::new("shift-up", SelectUp, Some("TextInput")),
        KeyBinding::new("shift-down", SelectDown, Some("TextInput")),
        KeyBinding::new("shift-enter", Newline, Some("TextInput")),
        KeyBinding::new("cmd-a", SelectAll, Some("TextInput")),
        KeyBinding::new("cmd-v", Paste, Some("TextInput")),
        KeyBinding::new("cmd-c", Copy, Some("TextInput")),
        KeyBinding::new("cmd-x", Cut, Some("TextInput")),
        KeyBinding::new("home", Home, Some("TextInput")),
        KeyBinding::new("end", End, Some("TextInput")),
        KeyBinding::new("ctrl-cmd-space", ShowCharacterPalette, Some("TextInput")),
    ]
}

enum LastLayout {
    Single(Box<ShapedLine>),
    Multi(InputLayout),
}

struct InputLayout {
    paragraphs: Vec<LaidParagraph>,
    line_height: Pixels,
    placeholder: bool,
}

struct LaidParagraph {
    start: usize,
    line: WrappedLine,
}

pub struct TextInput {
    focus_handle: FocusHandle,
    content: SharedString,
    placeholder: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Option<LastLayout>,
    last_bounds: Option<Bounds<Pixels>>,
    last_visual_line_count: usize,
    /// Logical line count that `last_visual_line_count` was measured against.
    /// When this differs from the current text, height follows the text immediately
    /// instead of waiting a frame for paint.
    shaped_logical_lines: usize,
    preferred_x: Option<Pixels>,
    is_selecting: bool,
    multiline: bool,
    /// Set after the first paint. GPUI 0.2.2 needs one frame with a dispatch
    /// tree before IME is safe; after that the handler must stay registered.
    ime_handler_ready: bool,
}

impl TextInput {
    pub fn new(placeholder: impl Into<SharedString>, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            content: "".into(),
            placeholder: placeholder.into(),
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
            last_visual_line_count: 1,
            shaped_logical_lines: 1,
            preferred_x: None,
            is_selecting: false,
            multiline: false,
            ime_handler_ready: false,
        }
    }

    pub fn multiline(mut self) -> Self {
        self.multiline = true;
        self
    }

    pub fn text(&self) -> &str {
        &self.content
    }

    pub fn focus_handle_clone(&self) -> FocusHandle {
        self.focus_handle.clone()
    }

    pub fn is_composing(&self) -> bool {
        self.marked_range
            .as_ref()
            .is_some_and(|range| range.start < range.end)
    }

    pub fn extra_height(&self) -> Pixels {
        if !self.multiline {
            return px(0.);
        }
        INPUT_LINE_HEIGHT * self.visible_line_count().saturating_sub(1) as f32
    }

    pub fn is_expanded(&self) -> bool {
        self.multiline && self.visible_line_count() > 1
    }

    fn visible_line_count(&self) -> usize {
        self.layout_line_count().clamp(1, MAX_VISIBLE_LINES)
    }

    fn layout_line_count(&self) -> usize {
        if !self.multiline {
            return 1;
        }
        display_line_count(
            logical_line_count(&self.content),
            self.shaped_logical_lines,
            self.last_visual_line_count,
        )
    }

    pub fn reset(&mut self) {
        self.content = "".into();
        self.selected_range = 0..0;
        self.selection_reversed = false;
        self.marked_range = None;
        self.is_selecting = false;
        self.preferred_x = None;
        self.last_visual_line_count = 1;
        self.shaped_logical_lines = 1;
        // Keep last_layout / ime_handler_ready. Clearing them unregisters the
        // macOS IME client for a frame, and Japanese input often stays off.
    }

    pub fn set_text(&mut self, text: impl Into<SharedString>) {
        let content: SharedString = text.into();
        let len = content.len();
        self.content = content;
        self.selected_range = len..len;
        self.selection_reversed = false;
        self.marked_range = None;
        self.is_selecting = false;
        self.preferred_x = None;
        self.last_visual_line_count = logical_line_count(&self.content).max(1);
        self.shaped_logical_lines = self.last_visual_line_count;
    }

    pub fn set_placeholder(&mut self, placeholder: impl Into<SharedString>) {
        self.placeholder = placeholder.into();
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        self.preferred_x = None;
        if self.selected_range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.start, cx);
        }
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        self.preferred_x = None;
        if self.selected_range.is_empty() {
            self.move_to(self.next_boundary(self.selected_range.end), cx);
        } else {
            self.move_to(self.selected_range.end, cx);
        }
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.preferred_x = None;
        self.select_to(self.previous_boundary(self.cursor_offset()), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.preferred_x = None;
        self.select_to(self.next_boundary(self.cursor_offset()), cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.preferred_x = None;
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx);
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.preferred_x = None;
        if self.multiline {
            let start = self.visual_line_start(self.cursor_offset());
            self.move_to(start, cx);
        } else {
            self.move_to(0, cx);
        }
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.preferred_x = None;
        if self.multiline {
            let end = self.visual_line_end(self.cursor_offset());
            self.move_to(end, cx);
        } else {
            self.move_to(self.content.len(), cx);
        }
    }

    fn up(&mut self, _: &Up, _: &mut Window, cx: &mut Context<Self>) {
        self.move_vertically(-1, false, cx);
    }

    fn down(&mut self, _: &Down, _: &mut Window, cx: &mut Context<Self>) {
        self.move_vertically(1, false, cx);
    }

    fn select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        self.move_vertically(-1, true, cx);
    }

    fn select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        self.move_vertically(1, true, cx);
    }

    fn newline(&mut self, _: &Newline, window: &mut Window, cx: &mut Context<Self>) {
        if !self.multiline || self.is_composing() {
            return;
        }
        self.preferred_x = None;
        self.replace_text_in_range(None, "\n", window, cx);
    }

    fn move_vertically(&mut self, direction: isize, selecting: bool, cx: &mut Context<Self>) {
        if self.is_composing() {
            cx.propagate();
            return;
        }
        if direction < 0 && self.content.is_empty() && !selecting {
            cx.propagate();
            return;
        }
        if !self.multiline {
            if direction < 0 && !selecting {
                cx.propagate();
            }
            return;
        }
        let Some(bounds) = self.last_bounds else {
            return;
        };
        let Some(LastLayout::Multi(layout)) = self.last_layout.as_ref() else {
            return;
        };
        if layout.placeholder {
            if direction < 0 && !selecting {
                cx.propagate();
            }
            return;
        }
        let cursor = self.cursor_offset();
        let pos = position_for_doc_index(layout, cursor);
        if direction < 0 && pos.y < layout.line_height * 0.5 {
            return;
        }
        let x = self.preferred_x.unwrap_or(pos.x);
        self.preferred_x = Some(x);
        let target = point(
            bounds.left() + x,
            bounds.top() + pos.y + layout.line_height * direction as f32 + layout.line_height * 0.5,
        );
        let index = self.index_for_mouse_position(target);
        if selecting {
            self.select_to(index, cx);
        } else {
            self.move_to(index, cx);
        }
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.previous_boundary(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.next_boundary(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.is_selecting = true;
        self.preferred_x = None;
        if event.modifiers.shift {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        } else {
            self.move_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _window: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    fn show_character_palette(
        &mut self,
        _: &ShowCharacterPalette,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.show_character_palette();
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace_text_in_range(None, &sanitize_insert(&text, self.multiline), window, cx);
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        let range = clamp_byte_range(&self.content, self.selected_range.clone());
        if range.start < range.end
            && let Some(text) = self.content.get(range)
        {
            cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        let range = clamp_byte_range(&self.content, self.selected_range.clone());
        if range.start < range.end
            && let Some(text) = self.content.get(range)
        {
            cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
            self.replace_text_in_range(None, "", window, cx);
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        let offset = clamp_to_boundary(&self.content, offset);
        self.selected_range = offset..offset;
        cx.notify();
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        if self.content.is_empty() {
            return 0;
        }
        let (Some(bounds), Some(layout)) = (self.last_bounds.as_ref(), self.last_layout.as_ref())
        else {
            return 0;
        };
        match layout {
            LastLayout::Single(line) => {
                if position.y < bounds.top() {
                    return 0;
                }
                if position.y > bounds.bottom() {
                    return self.content.len();
                }
                line.closest_index_for_x(position.x - bounds.left())
            }
            LastLayout::Multi(layout) => {
                if layout.placeholder {
                    return 0;
                }
                index_for_doc_position(layout, *bounds, position, self.content.len())
            }
        }
    }

    fn visual_line_start(&self, index: usize) -> usize {
        let Some(LastLayout::Multi(layout)) = self.last_layout.as_ref() else {
            return 0;
        };
        visual_line_bounds(layout, index).start
    }

    fn visual_line_end(&self, index: usize) -> usize {
        let Some(LastLayout::Multi(layout)) = self.last_layout.as_ref() else {
            return self.content.len();
        };
        visual_line_bounds(layout, index).end
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset;
        } else {
            self.selected_range.end = offset;
        }
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        cx.notify();
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;
        for ch in self.content.chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }
        utf8_offset
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;
        for ch in self.content.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }
        utf16_offset
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range_utf16.start)..self.offset_from_utf16(range_utf16.end)
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(idx, _)| (idx < offset).then_some(idx))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .find_map(|(idx, _)| (idx > offset).then_some(idx))
            .unwrap_or(self.content.len())
    }
}

impl EntityInputHandler for TextInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = clamp_byte_range(&self.content, self.range_from_utf16(&range_utf16));
        actual_range.replace(self.range_to_utf16(&range));
        self.content.get(range).map(ToString::to_string)
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let range = clamp_byte_range(&self.content, self.selected_range.clone());
        Some(UTF16Selection {
            range: self.range_to_utf16(&range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let new_text = sanitize_insert(new_text, self.multiline);
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or_else(|| self.selected_range.clone());
        let range = replace_bytes(&mut self.content, range, &new_text);
        let caret = range.start + new_text.len();
        self.selected_range = caret..caret;
        self.marked_range.take();
        self.preferred_x = None;
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let new_text = sanitize_insert(new_text, self.multiline);
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or_else(|| self.selected_range.clone());
        let range = replace_bytes(&mut self.content, range, &new_text);
        if new_text.is_empty() {
            self.marked_range = None;
        } else {
            self.marked_range = Some(range.start..range.start + new_text.len());
        }
        // IME selectedRange is relative to the marked text, not the document.
        self.selected_range = match new_selected_range_utf16 {
            Some(sel) => {
                let relative = clamp_byte_range(
                    &new_text,
                    utf16_to_utf8(&new_text, sel.start)..utf16_to_utf8(&new_text, sel.end),
                );
                range.start + relative.start..range.start + relative.end
            }
            None => {
                let caret = range.start + new_text.len();
                caret..caret
            }
        };
        self.preferred_x = None;
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = clamp_byte_range(&self.content, self.range_from_utf16(&range_utf16));
        match self.last_layout.as_ref()? {
            LastLayout::Single(last_layout) => Some(Bounds::from_corners(
                point(
                    bounds.left() + last_layout.x_for_index(range.start),
                    bounds.top(),
                ),
                point(
                    bounds.left() + last_layout.x_for_index(range.end),
                    bounds.bottom(),
                ),
            )),
            LastLayout::Multi(layout) => {
                let start = position_for_doc_index(layout, range.start);
                let end = position_for_doc_index(layout, range.end);
                let line_height = layout.line_height;
                if start.y == end.y {
                    Some(Bounds::from_corners(
                        point(bounds.left() + start.x, bounds.top() + start.y),
                        point(
                            bounds.left() + end.x.max(start.x + px(2.)),
                            bounds.top() + start.y + line_height,
                        ),
                    ))
                } else {
                    Some(Bounds::from_corners(
                        point(bounds.left() + start.x, bounds.top() + start.y),
                        point(
                            bounds.left() + start.x + px(2.),
                            bounds.top() + start.y + line_height,
                        ),
                    ))
                }
            }
        }
    }

    fn character_index_for_point(
        &mut self,
        point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let utf8_index = match self.last_layout.as_ref()? {
            LastLayout::Single(last_layout) => {
                let line_point = self.last_bounds?.localize(&point)?;
                last_layout.index_for_x(point.x - line_point.x)?
            }
            LastLayout::Multi(layout) => {
                let bounds = self.last_bounds?;
                index_for_doc_position(layout, bounds, point, self.content.len())
            }
        };
        Some(self.offset_to_utf16(utf8_index))
    }
}

struct TextElement {
    input: Entity<TextInput>,
}

struct PrepaintState {
    single_line: Option<ShapedLine>,
    multi: Option<InputLayout>,
    cursor: Option<PaintQuad>,
    selection: Vec<PaintQuad>,
}

impl IntoElement for TextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let input = self.input.read(cx);
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        let lines = if input.multiline {
            input.layout_line_count() as f32
        } else {
            1.
        };
        style.size.height = (window.line_height() * lines).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let content = input.content.clone();
        let selected_range = input.selected_range.clone();
        let cursor = input.cursor_offset();
        let multiline = input.multiline;
        let style = window.text_style();
        let dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );

        let content_empty = content.is_empty();
        let (display_text, text_color) = if content_empty {
            let placeholder = if dark {
                hsla(0., 0., 1., 0.35)
            } else {
                hsla(0., 0., 0., 0.35)
            };
            (input.placeholder.clone(), placeholder)
        } else {
            (content, style.color)
        };

        let run = TextRun {
            len: display_text.len(),
            font: style.font(),
            color: text_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = if content_empty {
            vec![run]
        } else if let Some(marked_range) = input.marked_range.as_ref() {
            let marked = clamp_byte_range(display_text.as_ref(), marked_range.clone());
            if marked.end > display_text.len() || marked.start >= marked.end {
                vec![run]
            } else {
                vec![
                    TextRun {
                        len: marked.start,
                        ..run.clone()
                    },
                    TextRun {
                        len: marked.end - marked.start,
                        underline: Some(UnderlineStyle {
                            color: Some(run.color),
                            thickness: px(1.0),
                            wavy: false,
                        }),
                        ..run.clone()
                    },
                    TextRun {
                        len: display_text.len() - marked.end,
                        ..run
                    },
                ]
                .into_iter()
                .filter(|run| run.len > 0)
                .collect()
            }
        } else {
            vec![run]
        };

        let font_size = style.font_size.to_pixels(window.rem_size());
        let line_height = window.line_height();

        if !multiline {
            let line = window
                .text_system()
                .shape_line(display_text, font_size, &runs, None);
            let cursor_pos = line.x_for_index(cursor);
            let (selection, cursor) = if selected_range.is_empty() {
                (
                    Vec::new(),
                    Some(fill(
                        Bounds::new(
                            point(bounds.left() + cursor_pos, bounds.top()),
                            size(px(2.), bounds.bottom() - bounds.top()),
                        ),
                        rgb(0x0a84ff),
                    )),
                )
            } else {
                (
                    vec![fill(
                        Bounds::from_corners(
                            point(
                                bounds.left() + line.x_for_index(selected_range.start),
                                bounds.top(),
                            ),
                            point(
                                bounds.left() + line.x_for_index(selected_range.end),
                                bounds.bottom(),
                            ),
                        ),
                        rgba(0x0a84ff40),
                    )],
                    None,
                )
            };
            return PrepaintState {
                single_line: Some(line),
                multi: None,
                cursor,
                selection,
            };
        }

        let wrap_width = (bounds.size.width > px(0.)).then_some(bounds.size.width);
        let paragraphs = window
            .text_system()
            .shape_text(display_text, font_size, &runs, wrap_width, None)
            .unwrap_or_else(|_| Default::default());
        let layout = InputLayout {
            paragraphs: wrapped_paragraphs(paragraphs),
            line_height,
            placeholder: content_empty,
        };
        let cursor_pos = position_for_doc_index(&layout, cursor);
        let selection = if layout.placeholder || selected_range.is_empty() {
            Vec::new()
        } else {
            selection_quads(&layout, bounds, &selected_range)
        };
        let cursor = if selected_range.is_empty() {
            Some(fill(
                Bounds::new(
                    point(bounds.left() + cursor_pos.x, bounds.top() + cursor_pos.y),
                    size(px(2.), line_height),
                ),
                rgb(0x0a84ff),
            ))
        } else {
            None
        };
        PrepaintState {
            single_line: None,
            multi: Some(layout),
            cursor,
            selection,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        let ime_handler_ready = self.input.read(cx).ime_handler_ready;
        if ime_handler_ready {
            window.handle_input(
                &focus_handle,
                ElementInputHandler::new(bounds, self.input.clone()),
                cx,
            );
        } else {
            // GPUI 0.2.2 registers the IME handler at the end of draw, before
            // swapping in this frame's dispatch tree. Japanese IME then calls
            // doCommandBySelector, which dispatches a key against an empty tree
            // and aborts (`panic_cannot_unwind`). Skip only this first paint.
            window.request_animation_frame();
        }
        for selection in prepaint.selection.drain(..) {
            window.paint_quad(selection);
        }
        let line_height = window.line_height();
        if let Some(line) = prepaint.single_line.take() {
            let _ = line.paint(bounds.origin, line_height, window, cx);
            self.input.update(cx, |input, _cx| {
                input.last_layout = Some(LastLayout::Single(Box::new(line)));
                input.last_bounds = Some(bounds);
                input.last_visual_line_count = 1;
                input.shaped_logical_lines = 1;
                input.ime_handler_ready = true;
            });
        } else if let Some(layout) = prepaint.multi.take() {
            let visual_lines = visual_line_count(&layout);
            let mut origin = bounds.origin;
            for para in &layout.paragraphs {
                let _ = para.line.paint(
                    origin,
                    line_height,
                    TextAlign::Left,
                    Some(bounds),
                    window,
                    cx,
                );
                origin.y += para.line.size(line_height).height;
            }
            self.input.update(cx, |input, cx| {
                let logical = logical_line_count(&input.content).max(1);
                let changed = input.last_visual_line_count != visual_lines
                    || input.shaped_logical_lines != logical;
                input.last_layout = Some(LastLayout::Multi(layout));
                input.last_bounds = Some(bounds);
                input.last_visual_line_count = visual_lines;
                input.shaped_logical_lines = logical;
                input.ime_handler_ready = true;
                if changed {
                    cx.notify();
                }
            });
        }

        if focus_handle.is_focused(window)
            && let Some(cursor) = prepaint.cursor.take()
        {
            window.paint_quad(cursor);
        }
    }
}

impl Render for TextInput {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let expanded = self.is_expanded();
        let visible_h = INPUT_LINE_HEIGHT * self.visible_line_count() as f32;
        div()
            .id(("text-input", cx.entity_id()))
            .flex()
            .key_context("TextInput")
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::newline))
            .on_action(cx.listener(Self::show_character_palette))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .w_full()
            .line_height(INPUT_LINE_HEIGHT)
            .text_size(px(18.))
            .when(expanded, |this| this.h(visible_h).overflow_y_scroll())
            .child(TextElement { input: cx.entity() })
    }
}

impl Focusable for TextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

fn wrapped_paragraphs(lines: impl IntoIterator<Item = WrappedLine>) -> Vec<LaidParagraph> {
    let mut start = 0;
    let mut paragraphs = Vec::new();
    for line in lines {
        let len = line.len();
        paragraphs.push(LaidParagraph { start, line });
        start += len + 1;
    }
    if paragraphs.is_empty() {
        paragraphs.push(LaidParagraph {
            start: 0,
            line: WrappedLine::default(),
        });
    }
    paragraphs
}

fn visual_line_count(layout: &InputLayout) -> usize {
    layout
        .paragraphs
        .iter()
        .map(|para| para.line.wrap_boundaries().len() + 1)
        .sum::<usize>()
        .max(1)
}

fn wrapped_row_starts(line: &WrappedLine) -> Vec<usize> {
    let mut starts = vec![0];
    for boundary in line.wrap_boundaries() {
        let Some(run) = line.runs().get(boundary.run_ix) else {
            continue;
        };
        let Some(glyph) = run.glyphs.get(boundary.glyph_ix) else {
            continue;
        };
        if glyph.index > *starts.last().unwrap_or(&0) {
            starts.push(glyph.index);
        }
    }
    starts
}

fn visual_line_bounds(layout: &InputLayout, index: usize) -> Range<usize> {
    for (para_ix, para) in layout.paragraphs.iter().enumerate() {
        let is_last = para_ix + 1 == layout.paragraphs.len();
        let content_end = para.start + para.line.len();
        let owned_end = if is_last {
            content_end
        } else {
            content_end + 1
        };
        if index < owned_end || is_last {
            let local = index.saturating_sub(para.start).min(para.line.len());
            let starts = wrapped_row_starts(&para.line);
            for (row_ix, start) in starts.iter().enumerate() {
                let next = starts.get(row_ix + 1).copied().unwrap_or(para.line.len());
                if local < next || row_ix + 1 == starts.len() {
                    let abs_start = para.start + start;
                    let abs_end = if row_ix + 1 == starts.len() && !is_last {
                        content_end
                    } else {
                        para.start + next
                    };
                    return abs_start..abs_end;
                }
            }
            return para.start..content_end;
        }
    }
    0..0
}

fn position_for_doc_index(layout: &InputLayout, index: usize) -> Point<Pixels> {
    let mut y = px(0.);
    for (para_ix, para) in layout.paragraphs.iter().enumerate() {
        let height = para.line.size(layout.line_height).height;
        let is_last = para_ix + 1 == layout.paragraphs.len();
        let content_end = para.start + para.line.len();
        let owned_end = if is_last {
            content_end
        } else {
            content_end + 1
        };
        if index < owned_end || is_last {
            let local = if index >= content_end {
                para.line.len()
            } else {
                index.saturating_sub(para.start)
            };
            let pos = para
                .line
                .position_for_index(local, layout.line_height)
                .unwrap_or(point(px(0.), height - layout.line_height));
            return point(pos.x, y + pos.y);
        }
        y += height;
    }
    point(px(0.), y)
}

fn index_for_doc_position(
    layout: &InputLayout,
    bounds: Bounds<Pixels>,
    position: Point<Pixels>,
    content_len: usize,
) -> usize {
    if layout.placeholder {
        return 0;
    }
    if position.y < bounds.top() {
        return 0;
    }
    let mut y = bounds.top();
    let local_x = position.x - bounds.left();
    for (para_ix, para) in layout.paragraphs.iter().enumerate() {
        let height = para.line.size(layout.line_height).height;
        let next_y = y + height;
        let last = para_ix + 1 == layout.paragraphs.len();
        if position.y < next_y || last {
            let local = point(local_x, (position.y - y).max(px(0.)));
            let idx = match para
                .line
                .closest_index_for_position(local, layout.line_height)
            {
                Ok(idx) | Err(idx) => idx,
            };
            return (para.start + idx).min(content_len);
        }
        y = next_y;
    }
    content_len
}

fn selection_quads(
    layout: &InputLayout,
    bounds: Bounds<Pixels>,
    selected: &Range<usize>,
) -> Vec<PaintQuad> {
    let mut quads = Vec::new();
    let mut y = bounds.top();
    for (para_ix, para) in layout.paragraphs.iter().enumerate() {
        let is_last = para_ix + 1 == layout.paragraphs.len();
        let content_end = para.start + para.line.len();
        let owned_end = if is_last {
            content_end
        } else {
            content_end + 1
        };
        let height = para.line.size(layout.line_height).height;
        let overlap_start = selected.start.max(para.start);
        let overlap_end = selected.end.min(owned_end);
        if overlap_start < overlap_end {
            let starts = wrapped_row_starts(&para.line);
            for (row_ix, start) in starts.iter().enumerate() {
                let next = starts.get(row_ix + 1).copied().unwrap_or(para.line.len());
                let row_start = para.start + start;
                let row_end = para.start + next;
                let last_row = row_ix + 1 == starts.len();
                let sel_start = overlap_start.max(row_start);
                let mut sel_end = overlap_end.min(if last_row { owned_end } else { row_end });
                if sel_start >= sel_end && !(last_row && overlap_end > content_end) {
                    continue;
                }
                let local_start = sel_start.saturating_sub(para.start).min(para.line.len());
                if last_row && overlap_end > content_end {
                    sel_end = content_end;
                }
                let local_end = sel_end.saturating_sub(para.start).min(para.line.len());
                let x0 = para.line.unwrapped_x_for_index(local_start, *start);
                let mut x1 = para.line.unwrapped_x_for_index(local_end, *start);
                if last_row && overlap_end > content_end {
                    x1 = x1.max(x0) + px(6.);
                }
                if x1 < x0 {
                    continue;
                }
                let row_y = y + layout.line_height * row_ix as f32;
                quads.push(fill(
                    Bounds::from_corners(
                        point(bounds.left() + x0, row_y),
                        point(
                            bounds.left() + x1.max(x0 + px(2.)),
                            row_y + layout.line_height,
                        ),
                    ),
                    rgba(0x0a84ff40),
                ));
            }
        }
        y += height;
    }
    quads
}

trait UnwrappedX {
    fn unwrapped_x_for_index(&self, index: usize, row_start: usize) -> Pixels;
}

impl UnwrappedX for WrappedLine {
    fn unwrapped_x_for_index(&self, index: usize, row_start: usize) -> Pixels {
        let x = self.unwrapped_layout.x_for_index(index);
        let start_x = self.unwrapped_layout.x_for_index(row_start);
        x - start_x
    }
}

fn logical_line_count(text: &str) -> usize {
    if text.is_empty() {
        1
    } else {
        text.bytes().filter(|&b| b == b'\n').count() + 1
    }
}

fn display_line_count(logical: usize, shaped_logical: usize, last_visual: usize) -> usize {
    let logical = logical.max(1);
    if shaped_logical == logical {
        last_visual.max(logical)
    } else {
        logical
    }
}

fn sanitize_insert(text: &str, multiline: bool) -> String {
    let normalized = normalize_newlines(text);
    if multiline {
        normalized
    } else {
        normalized.replace('\n', " ")
    }
}

fn normalize_newlines(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn clamp_to_boundary(text: &str, mut offset: usize) -> usize {
    if offset >= text.len() {
        return text.len();
    }
    while offset > 0 && !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn clamp_byte_range(text: &str, range: Range<usize>) -> Range<usize> {
    let start = clamp_to_boundary(text, range.start);
    let end = clamp_to_boundary(text, range.end);
    start.min(end)..start.max(end)
}

fn utf16_to_utf8(text: &str, utf16_offset: usize) -> usize {
    let mut utf8 = 0;
    let mut utf16 = 0;
    for ch in text.chars() {
        if utf16 >= utf16_offset {
            break;
        }
        utf16 += ch.len_utf16();
        utf8 += ch.len_utf8();
    }
    utf8
}

fn replace_bytes(content: &mut SharedString, range: Range<usize>, new_text: &str) -> Range<usize> {
    let range = clamp_byte_range(content, range);
    let mut next =
        String::with_capacity(content.len() - (range.end - range.start) + new_text.len());
    next.push_str(&content[..range.start]);
    next.push_str(new_text);
    next.push_str(&content[range.end..]);
    *content = next.into();
    range
}

#[cfg(test)]
mod tests {
    use super::{
        clamp_byte_range, display_line_count, logical_line_count, sanitize_insert, utf16_to_utf8,
    };
    use std::ops::Range;

    #[test]
    fn clamps_mid_character_offsets() {
        let text = "あい";
        assert_eq!(clamp_byte_range(text, 1..4), 0..3);
        assert_eq!(clamp_byte_range(text, 0..100), 0..text.len());
        assert_eq!(
            clamp_byte_range(text, Range { start: 8, end: 2 }),
            0..text.len()
        );
    }

    #[test]
    fn utf16_offsets_are_relative_to_marked_text() {
        let marked = "あ";
        assert_eq!(utf16_to_utf8(marked, 0), 0);
        assert_eq!(utf16_to_utf8(marked, 1), 3);
        assert_eq!(utf16_to_utf8("hiあ", 2), 2);
    }

    #[test]
    fn logical_line_count_includes_trailing_newline() {
        assert_eq!(logical_line_count(""), 1);
        assert_eq!(logical_line_count("hello"), 1);
        assert_eq!(logical_line_count("hello\n"), 2);
        assert_eq!(logical_line_count("a\nb\nc"), 3);
    }

    #[test]
    fn sanitize_keeps_newlines_only_when_multiline() {
        assert_eq!(sanitize_insert("a\r\nb\rc", true), "a\nb\nc");
        assert_eq!(sanitize_insert("a\r\nb\rc", false), "a b c");
    }

    #[test]
    fn height_tracks_newlines_without_waiting_for_paint() {
        assert_eq!(display_line_count(3, 3, 3), 3);
        assert_eq!(display_line_count(2, 3, 3), 2);
        assert_eq!(display_line_count(4, 3, 3), 4);
        assert_eq!(display_line_count(1, 1, 3), 3);
    }
}
