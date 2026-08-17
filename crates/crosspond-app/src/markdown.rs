//! Markdown rendering for assistant transcript text.
//!
//! Parses with pulldown-cmark (GFM tables, strikethrough, task lists) and
//! paints GPUI 0.2.2 elements. Confirm APIs against that crate; do not copy
//! later Zed `markdown` views.

use std::iter::Peekable;
use std::ops::Range;

use gpui::{
    AnyElement, App, ElementId, FontStyle, FontWeight, HighlightStyle, InteractiveText, Rgba,
    SharedString, StrikethroughStyle, StyledText, UnderlineStyle, combine_highlights, div,
    prelude::*, px, rems, rgb,
};
use pulldown_cmark::{
    Alignment as MdAlignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd,
};

#[derive(Clone, Copy)]
pub struct MarkdownPalette {
    pub text: Rgba,
    pub muted: Rgba,
    pub code_bg: Rgba,
    pub code_fg: Rgba,
    pub link: Rgba,
    pub border: Rgba,
    pub table_header_bg: Rgba,
    pub quote: Rgba,
}

impl MarkdownPalette {
    pub fn for_appearance(text: Rgba, muted: Rgba, dark: bool) -> Self {
        if dark {
            Self {
                text,
                muted,
                code_bg: rgb(0x2c2c2e),
                code_fg: rgb(0xe5e5ea),
                link: rgb(0x64d2ff),
                border: rgb(0x3a3a3c),
                table_header_bg: rgb(0x2c2c2e),
                quote: rgb(0x48484a),
            }
        } else {
            Self {
                text,
                muted,
                code_bg: rgb(0xf2f2f7),
                code_fg: rgb(0x1d1d1f),
                link: rgb(0x007aff),
                border: rgb(0xd2d2d7),
                table_header_bg: rgb(0xf2f2f7),
                quote: rgb(0xc7c7cc),
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Block {
    Paragraph(Inlines),
    Heading {
        level: u8,
        content: Inlines,
    },
    List {
        ordered: bool,
        start: u64,
        items: Vec<ListItem>,
    },
    Code {
        language: Option<String>,
        code: String,
    },
    Quote(Vec<Block>),
    Table {
        alignments: Vec<Align>,
        header: Vec<Inlines>,
        rows: Vec<Vec<Inlines>>,
    },
    Rule,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ListItem {
    checked: Option<bool>,
    blocks: Vec<Block>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Align {
    None,
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct Inlines {
    text: String,
    marks: Vec<Mark>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Mark {
    range: Range<usize>,
    kind: MarkKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum MarkKind {
    Bold,
    Italic,
    Code,
    Strikethrough,
    Link { href: String },
}

pub fn render(source: &str, palette: MarkdownPalette, id_seed: usize) -> AnyElement {
    let blocks = parse(source);
    let blocks = if blocks.is_empty() {
        let trimmed = source.trim();
        if trimmed.is_empty() {
            return div().into_any_element();
        }
        vec![Block::Paragraph(Inlines {
            text: trimmed.to_string(),
            marks: Vec::new(),
        })]
    } else {
        blocks
    };
    let text_color = palette.text;
    let mut ctx = RenderCtx {
        palette,
        id_seed,
        next_id: 0,
    };
    div()
        .flex()
        .flex_col()
        .flex_none()
        .w_full()
        .min_w_0()
        .gap_2()
        .text_sm()
        .line_height(rems(1.35))
        .text_color(text_color)
        .children(blocks.iter().map(|block| render_block(block, &mut ctx)))
        .into_any_element()
}

struct RenderCtx {
    palette: MarkdownPalette,
    id_seed: usize,
    next_id: usize,
}

impl RenderCtx {
    fn next_element_id(&mut self) -> ElementId {
        let n = self.next_id;
        self.next_id += 1;
        ElementId::from((SharedString::from(format!("md-{}", self.id_seed)), n))
    }
}

fn render_block(block: &Block, ctx: &mut RenderCtx) -> AnyElement {
    match block {
        Block::Paragraph(inlines) => render_inlines(inlines, ctx),
        Block::Heading { level, content } => {
            let heading = match level {
                1 => div().text_base().font_weight(FontWeight::BOLD),
                2 => div().text_sm().font_weight(FontWeight::BOLD),
                _ => div().text_sm().font_weight(FontWeight::SEMIBOLD),
            };
            heading
                .w_full()
                .min_w_0()
                .child(render_inlines(content, ctx))
                .into_any_element()
        }
        Block::List {
            ordered,
            start,
            items,
        } => render_list(*ordered, *start, items, ctx),
        Block::Code { language: _, code } => div()
            .w_full()
            .min_w_0()
            .rounded_sm()
            .bg(ctx.palette.code_bg)
            .px_2()
            .py_1()
            .font_family("Menlo")
            .text_xs()
            .text_color(ctx.palette.code_fg)
            .whitespace_normal()
            .child(code.clone())
            .into_any_element(),
        Block::Quote(inner) => div()
            .flex()
            .flex_row()
            .w_full()
            .min_w_0()
            .gap_2()
            .child(
                div()
                    .w(px(2.))
                    .flex_none()
                    .rounded_full()
                    .bg(ctx.palette.quote),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap_2()
                    .text_color(ctx.palette.muted)
                    .children(inner.iter().map(|block| render_block(block, ctx))),
            )
            .into_any_element(),
        Block::Table {
            alignments,
            header,
            rows,
        } => render_table(alignments, header, rows, ctx),
        Block::Rule => div()
            .w_full()
            .h(px(1.))
            .bg(ctx.palette.border)
            .into_any_element(),
    }
}

fn render_list(ordered: bool, start: u64, items: &[ListItem], ctx: &mut RenderCtx) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .w_full()
        .min_w_0()
        .gap_1()
        .children(items.iter().enumerate().map(|(index, item)| {
            let marker = list_marker(ordered, start, index, item.checked);
            div()
                .flex()
                .flex_row()
                .w_full()
                .min_w_0()
                .gap_2()
                .child(
                    div()
                        .flex_none()
                        .text_color(ctx.palette.muted)
                        .child(marker),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .gap_1()
                        .children(item.blocks.iter().map(|block| render_block(block, ctx))),
                )
                .into_any_element()
        }))
        .into_any_element()
}

fn list_marker(ordered: bool, start: u64, index: usize, checked: Option<bool>) -> String {
    if let Some(checked) = checked {
        return if checked { "☑".into() } else { "☐".into() };
    }
    if ordered {
        format!("{}.", start.saturating_add(index as u64))
    } else {
        "•".into()
    }
}

fn render_table(
    alignments: &[Align],
    header: &[Inlines],
    rows: &[Vec<Inlines>],
    ctx: &mut RenderCtx,
) -> AnyElement {
    let cols = alignments
        .len()
        .max(header.len())
        .max(rows.iter().map(Vec::len).max().unwrap_or(0));
    if cols == 0 {
        return div().into_any_element();
    }
    let mut all_rows: Vec<(bool, Vec<Inlines>)> = Vec::new();
    if !header.is_empty() {
        all_rows.push((true, pad_row(header, cols)));
    }
    for row in rows {
        all_rows.push((false, pad_row(row, cols)));
    }
    let min_width = px(72. * cols as f32);
    let row_count = all_rows.len();
    let border = ctx.palette.border;
    let scroll_id = ctx.next_element_id();
    div()
        .id(scroll_id)
        .w_full()
        .min_w_0()
        .overflow_x_scroll()
        .child(
            div()
                .flex()
                .flex_col()
                .w_full()
                .min_w(min_width)
                .border_1()
                .border_color(border)
                .rounded_sm()
                .children(all_rows.into_iter().enumerate().map(
                    |(row_index, (is_header, cells))| {
                        let alignments = alignments.to_vec();
                        render_table_row(
                            is_header,
                            cells,
                            &alignments,
                            cols,
                            row_index + 1 < row_count,
                            ctx,
                        )
                    },
                )),
        )
        .into_any_element()
}

fn pad_row(row: &[Inlines], cols: usize) -> Vec<Inlines> {
    let mut cells = row.to_vec();
    cells.resize(cols, Inlines::default());
    cells
}

fn render_table_row(
    is_header: bool,
    cells: Vec<Inlines>,
    alignments: &[Align],
    cols: usize,
    draw_bottom: bool,
    ctx: &mut RenderCtx,
) -> AnyElement {
    let mut row = div().flex().flex_row().w_full();
    if is_header {
        row = row
            .bg(ctx.palette.table_header_bg)
            .font_weight(FontWeight::SEMIBOLD);
    }
    row.children(cells.into_iter().enumerate().map(|(col, cell)| {
        let align = alignments.get(col).copied().unwrap_or(Align::None);
        let mut cell_div = div()
            .flex_1()
            .min_w(px(72.))
            .px_2()
            .py_1()
            .whitespace_normal();
        if col + 1 < cols {
            cell_div = cell_div.border_r_1().border_color(ctx.palette.border);
        }
        if draw_bottom {
            cell_div = cell_div.border_b_1().border_color(ctx.palette.border);
        }
        cell_div = match align {
            Align::Center => cell_div.text_center(),
            Align::Right => cell_div.text_right(),
            Align::Left | Align::None => cell_div.text_left(),
        };
        cell_div.child(render_inlines(&cell, ctx))
    }))
    .into_any_element()
}

fn render_inlines(inlines: &Inlines, ctx: &mut RenderCtx) -> AnyElement {
    if inlines.text.is_empty() {
        return div().into_any_element();
    }
    let highlights = flatten_marks(&inlines.marks, &ctx.palette, inlines.text.len());
    let styled = StyledText::new(inlines.text.clone()).with_highlights(highlights);
    let links: Vec<(Range<usize>, String)> = inlines
        .marks
        .iter()
        .filter_map(|mark| match &mark.kind {
            MarkKind::Link { href } => Some((mark.range.clone(), href.clone())),
            _ => None,
        })
        .collect();
    let body = if links.is_empty() {
        styled.into_any_element()
    } else {
        let ranges: Vec<Range<usize>> = links.iter().map(|(range, _)| range.clone()).collect();
        let hrefs: Vec<String> = links.into_iter().map(|(_, href)| href).collect();
        InteractiveText::new(ctx.next_element_id(), styled)
            .on_click(ranges, move |index, _, cx| {
                if let Some(href) = hrefs.get(index) {
                    open_safe_url(href, cx);
                }
            })
            .into_any_element()
    };
    div()
        .w_full()
        .min_w_0()
        .whitespace_normal()
        .child(body)
        .into_any_element()
}

fn flatten_marks(
    marks: &[Mark],
    palette: &MarkdownPalette,
    text_len: usize,
) -> Vec<(Range<usize>, HighlightStyle)> {
    marks.iter().fold(Vec::new(), |acc, mark| {
        let Some(range) = clamp_range(&mark.range, text_len) else {
            return acc;
        };
        combine_highlights(acc, [(range, style_for(&mark.kind, palette))]).collect()
    })
}

fn clamp_range(range: &Range<usize>, len: usize) -> Option<Range<usize>> {
    let start = range.start.min(len);
    let end = range.end.min(len);
    if start < end { Some(start..end) } else { None }
}

fn style_for(kind: &MarkKind, palette: &MarkdownPalette) -> HighlightStyle {
    match kind {
        MarkKind::Bold => FontWeight::BOLD.into(),
        MarkKind::Italic => FontStyle::Italic.into(),
        MarkKind::Code => HighlightStyle {
            color: Some(palette.code_fg.into()),
            background_color: Some(palette.code_bg.into()),
            ..Default::default()
        },
        MarkKind::Strikethrough => HighlightStyle {
            strikethrough: Some(StrikethroughStyle {
                thickness: px(1.),
                color: None,
            }),
            ..Default::default()
        },
        MarkKind::Link { .. } => HighlightStyle {
            color: Some(palette.link.into()),
            underline: Some(UnderlineStyle {
                thickness: px(1.),
                color: Some(palette.link.into()),
                wavy: false,
            }),
            ..Default::default()
        },
    }
}

fn open_safe_url(url: &str, cx: &mut App) {
    let Some((scheme, _)) = url.split_once(':') else {
        return;
    };
    if scheme.eq_ignore_ascii_case("http")
        || scheme.eq_ignore_ascii_case("https")
        || scheme.eq_ignore_ascii_case("mailto")
    {
        cx.open_url(url);
    }
}

fn parse_options() -> Options {
    Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS
}

fn parse(input: &str) -> Vec<Block> {
    let parser = Parser::new_ext(input, parse_options());
    let mut events = parser.peekable();
    parse_blocks(&mut events, None)
}

fn parse_blocks<'a, I>(events: &mut Peekable<I>, until: Option<TagEnd>) -> Vec<Block>
where
    I: Iterator<Item = Event<'a>>,
{
    let mut blocks = Vec::new();
    while let Some(event) = events.next() {
        match event {
            Event::End(tag) if until == Some(tag) => break,
            Event::End(_) => break,
            Event::Rule => blocks.push(Block::Rule),
            Event::Start(tag) => {
                let end = tag.to_end();
                match tag {
                    Tag::Paragraph => {
                        let content = parse_inlines(events, end);
                        if !content.text.trim().is_empty() {
                            blocks.push(Block::Paragraph(content));
                        }
                    }
                    Tag::Heading { level, .. } => {
                        let content = parse_inlines(events, end);
                        if !content.text.trim().is_empty() {
                            blocks.push(Block::Heading {
                                level: heading_level(level),
                                content,
                            });
                        }
                    }
                    Tag::List(start) => {
                        blocks.push(parse_list(events, start));
                    }
                    Tag::Item => {
                        let item = parse_list_item_after_start(events);
                        blocks.push(Block::List {
                            ordered: false,
                            start: 1,
                            items: vec![item],
                        });
                    }
                    Tag::BlockQuote(_) => {
                        let inner = parse_blocks(events, Some(end));
                        if !inner.is_empty() {
                            blocks.push(Block::Quote(inner));
                        }
                    }
                    Tag::CodeBlock(kind) => {
                        blocks.push(parse_code_block(events, kind));
                    }
                    Tag::Table(alignments) => {
                        blocks.push(parse_table(events, alignments));
                    }
                    Tag::HtmlBlock => {
                        drain_until(events, end);
                    }
                    Tag::Strong
                    | Tag::Emphasis
                    | Tag::Strikethrough
                    | Tag::Link { .. }
                    | Tag::Image { .. }
                    | Tag::Superscript
                    | Tag::Subscript => {
                        push_paragraph(&mut blocks, parse_inlines_after_start(events, tag));
                    }
                    _ => {
                        let mut inner = parse_blocks(events, Some(end));
                        blocks.append(&mut inner);
                    }
                }
            }
            Event::Text(_) | Event::Code(_) | Event::SoftBreak | Event::HardBreak => {
                push_paragraph(&mut blocks, parse_inlines_from(event, events));
            }
            _ => {}
        }
    }
    blocks
}

fn push_paragraph(blocks: &mut Vec<Block>, content: Inlines) {
    if !content.text.trim().is_empty() {
        blocks.push(Block::Paragraph(content));
    }
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn parse_list<'a, I>(events: &mut Peekable<I>, start: Option<u64>) -> Block
where
    I: Iterator<Item = Event<'a>>,
{
    let ordered = start.is_some();
    let start_n = start.unwrap_or(1);
    let mut items = Vec::new();
    loop {
        match events.peek() {
            Some(Event::Start(Tag::Item)) => {
                events.next();
                items.push(parse_list_item_after_start(events));
            }
            Some(Event::End(TagEnd::List(_))) => {
                events.next();
                break;
            }
            Some(Event::End(_)) | None => break,
            _ => {
                events.next();
            }
        }
    }
    Block::List {
        ordered,
        start: start_n,
        items,
    }
}

fn parse_list_item_after_start<'a, I>(events: &mut Peekable<I>) -> ListItem
where
    I: Iterator<Item = Event<'a>>,
{
    let checked = if let Some(Event::TaskListMarker(checked)) = events.peek() {
        let checked = *checked;
        events.next();
        Some(checked)
    } else {
        None
    };
    ListItem {
        checked,
        blocks: parse_blocks(events, Some(TagEnd::Item)),
    }
}

fn parse_code_block<'a, I>(events: &mut Peekable<I>, kind: CodeBlockKind<'_>) -> Block
where
    I: Iterator<Item = Event<'a>>,
{
    let language = match kind {
        CodeBlockKind::Fenced(lang) => {
            let lang = lang.trim();
            if lang.is_empty() {
                None
            } else {
                Some(lang.to_string())
            }
        }
        CodeBlockKind::Indented => None,
    };
    let mut code = String::new();
    for event in events.by_ref() {
        match event {
            Event::Text(text) => code.push_str(&text),
            Event::End(TagEnd::CodeBlock) => break,
            Event::End(_) => break,
            _ => {}
        }
    }
    if code.ends_with('\n') {
        code.pop();
    }
    Block::Code { language, code }
}

fn parse_table<'a, I>(events: &mut Peekable<I>, alignments: Vec<MdAlignment>) -> Block
where
    I: Iterator<Item = Event<'a>>,
{
    let alignments: Vec<Align> = alignments.into_iter().map(map_align).collect();
    let mut header = Vec::new();
    let mut rows = Vec::new();
    while let Some(event) = events.next() {
        match event {
            Event::Start(Tag::TableHead) => {
                header = parse_table_cells(events, TagEnd::TableHead);
            }
            Event::Start(Tag::TableRow) => {
                rows.push(parse_table_cells(events, TagEnd::TableRow));
            }
            Event::End(TagEnd::Table) => break,
            Event::End(_) => break,
            _ => {}
        }
    }
    Block::Table {
        alignments,
        header,
        rows,
    }
}

fn parse_table_cells<'a, I>(events: &mut Peekable<I>, until: TagEnd) -> Vec<Inlines>
where
    I: Iterator<Item = Event<'a>>,
{
    let mut cells = Vec::new();
    while let Some(event) = events.next() {
        match event {
            Event::Start(Tag::TableCell) => {
                cells.push(parse_inlines(events, TagEnd::TableCell));
            }
            Event::Start(Tag::TableRow) => {
                cells.append(&mut parse_table_cells(events, TagEnd::TableRow));
            }
            Event::End(tag) if tag == until => break,
            Event::End(_) => break,
            _ => {}
        }
    }
    cells
}

fn map_align(alignment: MdAlignment) -> Align {
    match alignment {
        MdAlignment::None => Align::None,
        MdAlignment::Left => Align::Left,
        MdAlignment::Center => Align::Center,
        MdAlignment::Right => Align::Right,
    }
}

fn is_block_start(tag: &Tag<'_>) -> bool {
    matches!(
        tag,
        Tag::Paragraph
            | Tag::Heading { .. }
            | Tag::BlockQuote(_)
            | Tag::CodeBlock(_)
            | Tag::HtmlBlock
            | Tag::List(_)
            | Tag::Item
            | Tag::FootnoteDefinition(_)
            | Tag::DefinitionList
            | Tag::DefinitionListTitle
            | Tag::DefinitionListDefinition
            | Tag::Table(_)
            | Tag::TableHead
            | Tag::TableRow
            | Tag::TableCell
            | Tag::MetadataBlock(_)
    )
}

fn is_block_end(tag: TagEnd) -> bool {
    matches!(
        tag,
        TagEnd::Paragraph
            | TagEnd::Heading(_)
            | TagEnd::BlockQuote(_)
            | TagEnd::CodeBlock
            | TagEnd::HtmlBlock
            | TagEnd::List(_)
            | TagEnd::Item
            | TagEnd::FootnoteDefinition
            | TagEnd::DefinitionList
            | TagEnd::DefinitionListTitle
            | TagEnd::DefinitionListDefinition
            | TagEnd::Table
            | TagEnd::TableHead
            | TagEnd::TableRow
            | TagEnd::TableCell
            | TagEnd::MetadataBlock(_)
    )
}

fn parse_inlines_from<'a, I>(first: Event<'a>, events: &mut Peekable<I>) -> Inlines
where
    I: Iterator<Item = Event<'a>>,
{
    let mut text = String::new();
    let mut marks = Vec::new();
    let mut stack = Vec::new();
    consume_inline_event(first, events, &mut text, &mut marks, &mut stack);
    collect_inlines_until_block(events, &mut text, &mut marks, &mut stack);
    Inlines { text, marks }
}

fn parse_inlines_after_start<'a, I>(events: &mut Peekable<I>, tag: Tag<'a>) -> Inlines
where
    I: Iterator<Item = Event<'a>>,
{
    parse_inlines_from(Event::Start(tag), events)
}

fn collect_inlines_until_block<'a, I>(
    events: &mut Peekable<I>,
    text: &mut String,
    marks: &mut Vec<Mark>,
    stack: &mut Vec<(MarkKind, usize)>,
) where
    I: Iterator<Item = Event<'a>>,
{
    loop {
        match events.peek() {
            None => break,
            Some(Event::Rule) | Some(Event::TaskListMarker(_)) => break,
            Some(Event::Start(tag)) if is_block_start(tag) => break,
            Some(Event::End(tag)) if is_block_end(*tag) => break,
            _ => {}
        }
        let Some(event) = events.next() else { break };
        consume_inline_event(event, events, text, marks, stack);
    }
    close_open_marks(marks, stack, text.len());
}

fn consume_inline_event<'a, I>(
    event: Event<'a>,
    events: &mut Peekable<I>,
    text: &mut String,
    marks: &mut Vec<Mark>,
    stack: &mut Vec<(MarkKind, usize)>,
) where
    I: Iterator<Item = Event<'a>>,
{
    match event {
        Event::Start(Tag::Strong) => stack.push((MarkKind::Bold, text.len())),
        Event::End(TagEnd::Strong) => close_mark(marks, stack, text.len()),
        Event::Start(Tag::Emphasis) => stack.push((MarkKind::Italic, text.len())),
        Event::End(TagEnd::Emphasis) => close_mark(marks, stack, text.len()),
        Event::Start(Tag::Strikethrough) => stack.push((MarkKind::Strikethrough, text.len())),
        Event::End(TagEnd::Strikethrough) => close_mark(marks, stack, text.len()),
        Event::Start(Tag::Link { dest_url, .. }) => {
            stack.push((
                MarkKind::Link {
                    href: dest_url.into_string(),
                },
                text.len(),
            ));
        }
        Event::End(TagEnd::Link) => close_mark(marks, stack, text.len()),
        Event::Start(Tag::Image { dest_url, .. }) => {
            let alt = parse_inlines(events, TagEnd::Image);
            if alt.text.is_empty() {
                text.push_str(&dest_url);
            } else {
                text.push_str(&alt.text);
            }
        }
        Event::Text(chunk) => text.push_str(&chunk),
        Event::Code(code) => {
            let start = text.len();
            text.push_str(&code);
            if start < text.len() {
                marks.push(Mark {
                    range: start..text.len(),
                    kind: MarkKind::Code,
                });
            }
        }
        Event::SoftBreak => text.push(' '),
        Event::HardBreak => text.push('\n'),
        Event::InlineHtml(html) if html.to_ascii_lowercase().contains("<br") => {
            text.push('\n');
        }
        _ => {}
    }
}

fn parse_inlines<'a, I>(events: &mut Peekable<I>, until: TagEnd) -> Inlines
where
    I: Iterator<Item = Event<'a>>,
{
    let mut text = String::new();
    let mut marks = Vec::new();
    let mut stack: Vec<(MarkKind, usize)> = Vec::new();
    while let Some(event) = events.next() {
        match event {
            Event::End(tag) if tag == until => {
                close_open_marks(&mut marks, &mut stack, text.len());
                break;
            }
            Event::Start(Tag::Strong) => stack.push((MarkKind::Bold, text.len())),
            Event::End(TagEnd::Strong) => close_mark(&mut marks, &mut stack, text.len()),
            Event::Start(Tag::Emphasis) => stack.push((MarkKind::Italic, text.len())),
            Event::End(TagEnd::Emphasis) => close_mark(&mut marks, &mut stack, text.len()),
            Event::Start(Tag::Strikethrough) => {
                stack.push((MarkKind::Strikethrough, text.len()));
            }
            Event::End(TagEnd::Strikethrough) => close_mark(&mut marks, &mut stack, text.len()),
            Event::Start(Tag::Link { dest_url, .. }) => {
                stack.push((
                    MarkKind::Link {
                        href: dest_url.into_string(),
                    },
                    text.len(),
                ));
            }
            Event::End(TagEnd::Link) => close_mark(&mut marks, &mut stack, text.len()),
            Event::Start(Tag::Image { dest_url, .. }) => {
                let alt = parse_inlines(events, TagEnd::Image);
                if alt.text.is_empty() {
                    text.push_str(&dest_url);
                } else {
                    text.push_str(&alt.text);
                }
            }
            Event::Text(chunk) => text.push_str(&chunk),
            Event::Code(code) => {
                let start = text.len();
                text.push_str(&code);
                if start < text.len() {
                    marks.push(Mark {
                        range: start..text.len(),
                        kind: MarkKind::Code,
                    });
                }
            }
            Event::SoftBreak => text.push(' '),
            Event::HardBreak => text.push('\n'),
            Event::InlineHtml(html) if html.to_ascii_lowercase().contains("<br") => {
                text.push('\n');
            }
            Event::End(_) => {
                close_open_marks(&mut marks, &mut stack, text.len());
                break;
            }
            _ => {}
        }
    }
    Inlines { text, marks }
}

fn close_mark(marks: &mut Vec<Mark>, stack: &mut Vec<(MarkKind, usize)>, end: usize) {
    if let Some((kind, start)) = stack.pop()
        && start < end
    {
        marks.push(Mark {
            range: start..end,
            kind,
        });
    }
}

fn close_open_marks(marks: &mut Vec<Mark>, stack: &mut Vec<(MarkKind, usize)>, end: usize) {
    while !stack.is_empty() {
        close_mark(marks, stack, end);
    }
}

fn drain_until<'a, I>(events: &mut Peekable<I>, until: TagEnd)
where
    I: Iterator<Item = Event<'a>>,
{
    let mut depth = 1;
    for event in events.by_ref() {
        match event {
            Event::Start(tag) if tag.to_end() == until => depth += 1,
            Event::End(tag) if tag == until => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paragraph_text(block: &Block) -> &str {
        match block {
            Block::Paragraph(inlines) => &inlines.text,
            other => panic!("{other:?}"),
        }
    }

    fn has_mark(inlines: &Inlines, kind_matches: impl Fn(&MarkKind) -> bool) -> bool {
        inlines.marks.iter().any(|mark| kind_matches(&mark.kind))
    }

    fn marked_text(inlines: &Inlines, kind_matches: impl Fn(&MarkKind) -> bool) -> String {
        inlines
            .marks
            .iter()
            .filter(|mark| kind_matches(&mark.kind))
            .map(|mark| inlines.text[mark.range.clone()].to_string())
            .collect::<Vec<_>>()
            .join("|")
    }

    #[test]
    fn parses_bold_lists_and_inline_code() {
        let blocks = parse(
            "`cordiverse/paper` は **時空間的結合性** を扱う。\n\n- **時間的結合性**\n- **空間的結合性**\n",
        );
        assert!(matches!(&blocks[0], Block::Paragraph(_)));
        let Block::Paragraph(first) = &blocks[0] else {
            panic!();
        };
        assert!(first.text.contains("cordiverse/paper"));
        assert_eq!(
            marked_text(first, |kind| matches!(kind, MarkKind::Code)),
            "cordiverse/paper"
        );
        assert!(
            marked_text(first, |kind| matches!(kind, MarkKind::Bold)).contains("時空間的結合性")
        );
        match &blocks[1] {
            Block::List {
                ordered: false,
                items,
                ..
            } => {
                assert_eq!(items.len(), 2);
                let Block::Paragraph(item) = &items[0].blocks[0] else {
                    panic!("{:?}", items[0].blocks);
                };
                assert!(has_mark(item, |kind| matches!(kind, MarkKind::Bold)));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn parses_gfm_tables() {
        let blocks = parse("| Name | Role |\n| --- | ---: |\n| Ada | Lead |\n| Bob | Support |\n");
        match &blocks[0] {
            Block::Table {
                alignments,
                header,
                rows,
            } => {
                assert_eq!(alignments, &[Align::None, Align::Right]);
                assert_eq!(header.len(), 2);
                assert_eq!(header[0].text, "Name");
                assert_eq!(header[1].text, "Role");
                assert_eq!(rows.len(), 2);
                assert_eq!(rows[0][0].text, "Ada");
                assert_eq!(rows[1][1].text, "Support");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn parses_fenced_code_and_links() {
        let blocks = parse("See [docs](https://example.com).\n\n```rust\nfn main() {}\n```\n");
        let Block::Paragraph(para) = &blocks[0] else {
            panic!("{:?}", blocks[0]);
        };
        assert_eq!(para.text, "See docs.");
        assert!(has_mark(para, |kind| matches!(
            kind,
            MarkKind::Link { href } if href == "https://example.com"
        )));
        match &blocks[1] {
            Block::Code { language, code } => {
                assert_eq!(language.as_deref(), Some("rust"));
                assert_eq!(code, "fn main() {}");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn parses_task_and_ordered_lists() {
        let blocks = parse("1. first\n2. second\n\n- [x] done\n- [ ] later\n");
        match &blocks[0] {
            Block::List {
                ordered: true,
                start,
                items,
            } => {
                assert_eq!(*start, 1);
                assert_eq!(items.len(), 2);
                assert_eq!(paragraph_text(&items[0].blocks[0]), "first");
            }
            other => panic!("{other:?}"),
        }
        match &blocks[1] {
            Block::List {
                ordered: false,
                items,
                ..
            } => {
                assert_eq!(items[0].checked, Some(true));
                assert_eq!(items[1].checked, Some(false));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn incomplete_markdown_does_not_panic() {
        let _ = parse("**unclosed");
        let _ = parse("| a | b |\n| ---");
        let _ = parse("```\nstill open");
        let _ = parse("");
        let _ = parse("plain text only");
    }

    #[test]
    fn skips_empty_input() {
        assert!(parse("").is_empty());
        assert!(parse("\n\n").is_empty());
    }
}
