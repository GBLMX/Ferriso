//! Drawing a [`Node`] tree into a ratatui buffer.
//!
//! Every interactive node records a hit region and a stable id. Input handling resolves the id
//! against a fresh view tree, so a click always invokes the callback from the current model
//! rather than a stale closure captured during the previous frame.

use std::collections::HashMap;

use ferriso::node::{
    Align, ButtonKind, Divider, Flex, ImageData, ImageFit, Length, List, Node, Progress,
    Qr, Scroll, Slider, Text, TextInput,
};
use ferriso::theme::{Rgb, Theme, Tone};
use ferriso::WidgetId;
use qrcode::QrCode;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use unicode_width::UnicodeWidthStr;

use crate::layout::{self, px_to_cells, scale_length, scale_padding, truncate, wrap_text};

pub const ROOT_ID: WidgetId = WidgetId(0x4665_7272_6973_6f);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HitKind {
    Button,
    TextInput,
    Slider,
    List,
    ListRow(usize),
}

#[derive(Clone, Debug)]
pub struct Hit {
    pub id: WidgetId,
    pub rect: Rect,
    pub kind: HitKind,
}

/// Focus, local editing state, and the hit map from the last frame.
pub struct UiState {
    pub(crate) focused: Option<WidgetId>,
    pub(crate) focus_order: Vec<WidgetId>,
    pub(crate) cursors: HashMap<WidgetId, usize>,
    pub(crate) scroll: HashMap<WidgetId, u16>,
    pub(crate) list_offsets: HashMap<WidgetId, usize>,
    pub(crate) hits: Vec<Hit>,
    pub(crate) scroll_regions: Vec<(WidgetId, Rect)>,
    pub(crate) mouse: Option<(u16, u16)>,
    pub(crate) quit: bool,
    pub(crate) slider_drag: Option<WidgetId>,
}

impl Default for UiState {
    fn default() -> Self {
        Self::new()
    }
}

impl UiState {
    pub fn new() -> Self {
        Self {
            focused: None,
            focus_order: Vec::new(),
            cursors: HashMap::new(),
            scroll: HashMap::new(),
            list_offsets: HashMap::new(),
            hits: Vec::new(),
            scroll_regions: Vec::new(),
            mouse: None,
            quit: false,
            slider_drag: None,
        }
    }

    pub fn focused(&self) -> Option<WidgetId> {
        self.focused
    }

    pub fn hit_at(&self, x: u16, y: u16) -> Option<&Hit> {
        self.hits
            .iter()
            .rev()
            .find(|hit| contains(hit.rect, x, y))
    }

    pub fn scroll_at(&self, x: u16, y: u16) -> Option<WidgetId> {
        self.scroll_regions
            .iter()
            .rev()
            .find(|(_, rect)| contains(*rect, x, y))
            .map(|(id, _)| *id)
    }

    pub(crate) fn begin_frame(&mut self) {
        self.focus_order.clear();
        self.hits.clear();
        self.scroll_regions.clear();
    }

    pub(crate) fn end_frame(&mut self) {
        if let Some(focused) = self.focused
            && !self.focus_order.contains(&focused)
        {
            self.focused = None;
        }
    }
}

fn contains(rect: Rect, x: u16, y: u16) -> bool {
    x >= rect.x
        && y >= rect.y
        && x < rect.x.saturating_add(rect.width)
        && y < rect.y.saturating_add(rect.height)
}

fn color(rgb: Rgb) -> Color {
    Color::Rgb(rgb.r, rgb.g, rgb.b)
}

pub(crate) fn explicit_id<Message>(node: &Node<'_, Message>) -> Option<WidgetId> {
    match node {
        Node::TextInput(input) => input.id,
        Node::Slider(slider) => slider.id,
        Node::List(list) => list.id,
        Node::Scroll(scroll) => scroll.id,
        _ => None,
    }
}

fn next_id<Message>(parent: WidgetId, index: usize, node: &Node<'_, Message>) -> WidgetId {
    if let Some(id) = explicit_id(node) {
        return id;
    }
    let mut value = parent
        .0
        .wrapping_add((index as u64).wrapping_add(1).wrapping_mul(0x9e37_79b9_7f4a_7c15));
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^= value >> 31;
    WidgetId(value)
}

fn fill_rect(buf: &mut Buffer, rect: Rect, rgb: Rgb) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    buf.set_style(rect, Style::default().bg(color(rgb)));
}

fn draw_border(buf: &mut Buffer, rect: Rect, rgb: Rgb, focused: bool) {
    if rect.width < 2 || rect.height < 2 {
        return;
    }
    let style = Style::default()
        .fg(color(rgb))
        .add_modifier(if focused {
            Modifier::BOLD
        } else {
            Modifier::empty()
        });
    let right = rect.x + rect.width - 1;
    let bottom = rect.y + rect.height - 1;
    for x in rect.x + 1..right {
        buf[(x, rect.y)].set_symbol("─").set_style(style);
        buf[(x, bottom)].set_symbol("─").set_style(style);
    }
    for y in rect.y + 1..bottom {
        buf[(rect.x, y)].set_symbol("│").set_style(style);
        buf[(right, y)].set_symbol("│").set_style(style);
    }
    buf[(rect.x, rect.y)].set_symbol("┌").set_style(style);
    buf[(right, rect.y)].set_symbol("┐").set_style(style);
    buf[(rect.x, bottom)].set_symbol("└").set_style(style);
    buf[(right, bottom)].set_symbol("┘").set_style(style);
}

fn draw_text(
    buf: &mut Buffer,
    area: Rect,
    text: &Text<'_>,
    theme: &Theme,
    forced: Option<Rgb>,
) {
    let area = clamp(area, buf.area);
    if area.width == 0 || area.height == 0 {
        return;
    }
    let rgb = if text.style.tone == Tone::Text {
        forced.unwrap_or(theme.text)
    } else {
        theme.resolve(text.style.tone)
    };
    let mut style = Style::default().fg(color(rgb));
    if text.style.weight == ferriso::node::Weight::Bold {
        style = style.add_modifier(Modifier::BOLD);
    }
    let lines = wrap_text(&text.content, area.width, text.style.wrap);
    for (index, line) in lines.iter().take(usize::from(area.height)).enumerate() {
        let width = u16::try_from(UnicodeWidthStr::width(line.as_str()))
            .unwrap_or(u16::MAX)
            .min(area.width);
        let spare = area.width.saturating_sub(width);
        let x = area.x
            + match text.style.align {
                Align::Start => 0,
                Align::Center => spare / 2,
                Align::End => spare,
            };
        buf.set_stringn(x, area.y + index as u16, line, usize::from(area.width), style);
    }
}

fn draw_flex<Message: Clone>(
    buf: &mut Buffer,
    area: Rect,
    flex: &Flex<'_, Message>,
    id: WidgetId,
    theme: &Theme,
    state: &mut UiState,
    forced: Option<Rgb>,
) {
    let area = clamp(area, buf.area);
    if area.width == 0 || area.height == 0 {
        return;
    }
    let vertical = matches!(
        &flex.direction,
        ferriso::node::Direction::Vertical
    );
    let focused = state.focused == Some(id);
    let padding = scale_padding(flex.padding);
    if let Some(fill) = flex.surface.fill {
        fill_rect(buf, area, theme.resolve(fill));
    }
    if let Some(border) = flex.surface.border {
        draw_border(
            buf,
            area,
            if focused {
                theme.accent
            } else {
                theme.resolve(border)
            },
            focused,
        );
    }
    let inner = layout::inset(area, padding, flex.surface.border.is_some());
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let gap = px_to_cells(flex.spacing);

    if vertical {
        let widths: Vec<u16> = flex
            .children
            .iter()
            .map(|child| cross_width(child, inner.width))
            .collect();
        let lengths: Vec<Length> = flex
            .children
            .iter()
            .map(|child| axis_length(child, true))
            .collect();
        let intrinsics: Vec<u16> = flex
            .children
            .iter()
            .zip(&widths)
            .map(|(child, width)| layout::intrinsic_height(child, *width))
            .collect();
        let sizes = layout::split(
            inner.height,
            gap,
            &lengths
                .iter()
                .copied()
                .zip(intrinsics)
                .collect::<Vec<_>>(),
        );
        let mut y = inner.y;
        for (index, ((child, width), height)) in flex
            .children
            .iter()
            .zip(widths)
            .zip(sizes)
            .enumerate()
        {
            let x = inner.x + layout::align_offset(inner.width, width, flex.align_x);
            let rect = Rect {
                x,
                y,
                width: width.min(inner.width),
                height: height.min(inner.height),
            };
            draw_node(
                buf,
                rect,
                child,
                next_id(id, index, child),
                theme,
                state,
                forced,
            );
            y = y.saturating_add(height).saturating_add(gap);
        }
    } else {
        let lengths: Vec<Length> = flex
            .children
            .iter()
            .map(|child| axis_length(child, false))
            .collect();
        let intrinsics: Vec<u16> = flex
            .children
            .iter()
            .map(|child| layout::intrinsic_width(child, inner.height))
            .collect();
        let sizes = layout::split(
            inner.width,
            gap,
            &lengths
                .iter()
                .copied()
                .zip(intrinsics)
                .collect::<Vec<_>>(),
        );
        let mut x = inner.x;
        for (index, (child, width)) in flex.children.iter().zip(sizes).enumerate() {
            let height = cross_height(child, width, inner.height);
            let y = inner.y + layout::align_offset(inner.height, height, flex.align_y);
            let rect = Rect {
                x,
                y,
                width: width.min(inner.width),
                height: height.min(inner.height),
            };
            draw_node(
                buf,
                rect,
                child,
                next_id(id, index, child),
                theme,
                state,
                forced,
            );
            x = x.saturating_add(width).saturating_add(gap);
        }
    }
}

/// Main-axis length. Columns split height; rows split width.
fn axis_length<Message>(node: &Node<'_, Message>, vertical: bool) -> Length {
    let length = if vertical {
        match node {
            Node::Column(flex) | Node::Row(flex) => flex.height,
            Node::Text(_) => Length::Shrink,
            Node::Button(button) => button.height,
            Node::TextInput(_) => Length::Shrink,
            Node::Slider(_) => Length::Fixed(24),
            Node::List(list) => list.height,
            Node::Scroll(scroll) => scroll.height,
            Node::Image(image) => image.height,
            Node::Qr(_) => Length::Shrink,
            Node::Progress(_) | Node::Divider(_) => Length::Shrink,
            Node::Space(space) => space.height,
        }
    } else {
        match node {
            Node::Column(flex) | Node::Row(flex) => flex.width,
            Node::Text(text) => text.style.width,
            Node::Button(button) => button.width,
            Node::TextInput(input) => input.width,
            Node::Slider(slider) => slider.width,
            Node::List(list) => list.width,
            Node::Scroll(scroll) => scroll.width,
            Node::Image(image) => image.width,
            Node::Qr(_) => Length::Shrink,
            Node::Progress(progress) => progress.width,
            Node::Divider(_) => Length::Shrink,
            Node::Space(space) => space.width,
        }
    };
    scale_length(length)
}

fn cross_width<Message>(node: &Node<'_, Message>, available: u16) -> u16 {
    let length = match node {
        Node::Column(flex) | Node::Row(flex) => flex.width,
        Node::Text(text) => text.style.width,
        Node::Button(button) => button.width,
        Node::TextInput(input) => input.width,
        Node::Slider(slider) => slider.width,
        Node::List(list) => list.width,
        Node::Scroll(scroll) => scroll.width,
        Node::Image(image) => image.width,
        Node::Qr(_) => Length::Shrink,
        Node::Progress(_) | Node::Divider(_) => Length::Shrink,
        Node::Space(space) => space.width,
    };
    match scale_length(length) {
        Length::Fixed(value) => value.min(available),
        Length::Shrink => layout::intrinsic_width(node, available).min(available),
        Length::Fill | Length::FillPortion(_) => available,
    }
}

fn cross_height<Message>(node: &Node<'_, Message>, width: u16, available: u16) -> u16 {
    let length = match node {
        Node::Column(flex) | Node::Row(flex) => flex.height,
        Node::Text(_) => Length::Shrink,
        Node::Button(button) => button.height,
        Node::TextInput(_) => Length::Shrink,
        Node::Slider(_) => Length::Fixed(24),
        Node::List(list) => list.height,
        Node::Scroll(scroll) => scroll.height,
        Node::Image(image) => image.height,
        Node::Qr(_) => Length::Shrink,
        Node::Progress(_) | Node::Divider(_) => Length::Shrink,
        Node::Space(space) => space.height,
    };
    match scale_length(length) {
        Length::Fixed(value) => value.min(available),
        Length::Shrink => layout::intrinsic_height(node, width).min(available),
        Length::Fill | Length::FillPortion(_) => available,
    }
}

fn draw_node<Message: Clone>(
    buf: &mut Buffer,
    area: Rect,
    node: &Node<'_, Message>,
    id: WidgetId,
    theme: &Theme,
    state: &mut UiState,
    forced: Option<Rgb>,
) {
    let area = clamp(area, buf.area);
    if area.width == 0 || area.height == 0 {
        return;
    }
    match node {
        Node::Column(flex) | Node::Row(flex) => {
            draw_flex(buf, area, flex, id, theme, state, forced)
        }
        Node::Text(text) => draw_text(buf, area, text, theme, forced),
        Node::Button(button) => {
            if button.on_press.is_some() {
                state.focus_order.push(id);
                state.hits.push(Hit {
                    id,
                    rect: area,
                    kind: HitKind::Button,
                });
            }
            let (fill, fg) = match button.kind {
                ButtonKind::Primary => (Some(theme.accent), Some(theme.on_accent)),
                ButtonKind::Danger => (Some(theme.danger), Some(theme.contrast_on(Tone::Danger))),
                ButtonKind::Secondary => (Some(theme.raised), Some(theme.text)),
                ButtonKind::Ghost => (None, forced.or(Some(theme.text))),
            };
            if let Some(fill) = fill {
                fill_rect(buf, area, fill);
            }
            let focused = state.focused == Some(id);
            let hovered = state
                .mouse
                .is_some_and(|(x, y)| contains(area, x, y) && button.on_press.is_some());
            if focused || hovered {
                draw_border(buf, area, theme.accent, focused);
            }
            let content_area = layout::inset(
                area,
                scale_padding(button.padding),
                false,
            );
            draw_node(buf, content_area, &button.content, next_id(id, 0, &button.content), theme, state, fg);
        }
        Node::TextInput(input) => {
            draw_input(buf, area, input, id, theme, state, forced)
        }
        Node::Slider(slider) => draw_slider(buf, area, slider, id, theme, state),
        Node::List(list) => draw_list(buf, area, list, id, theme, state),
        Node::Scroll(scroll) => draw_scroll(buf, area, scroll, id, theme, state),
        Node::Image(image) => draw_image(buf, area, image, theme, forced),
        Node::Qr(qr) => draw_qr(buf, area, qr, theme),
        Node::Progress(progress) => draw_progress(buf, area, progress, theme),
        Node::Space(_) => {}
        Node::Divider(divider) => draw_divider(buf, area, divider, theme),
    }
}

fn draw_input<Message: Clone>(
    buf: &mut Buffer,
    area: Rect,
    input: &TextInput<'_, Message>,
    id: WidgetId,
    theme: &Theme,
    state: &mut UiState,
    _forced: Option<Rgb>,
) {
    let focused = state.focused == Some(id);
    if input.on_input.is_some() {
        state.focus_order.push(id);
        state.hits.push(Hit {
            id,
            rect: area,
            kind: HitKind::TextInput,
        });
    }
    fill_rect(buf, area, theme.raised);
    draw_border(
        buf,
        area,
        if focused { theme.accent } else { theme.border },
        focused,
    );
    let inner = layout::inset(area, scale_padding(input.padding), true);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let value: String = if input.secure {
        input.value.chars().map(|_| '•').collect()
    } else {
        input.value.to_string()
    };
    let display = if value.is_empty() {
        input.placeholder.to_string()
    } else {
        value.clone()
    };
    let rgb = if value.is_empty() {
        theme.muted
    } else {
        theme.text
    };
    buf.set_stringn(
        inner.x,
        inner.y,
        truncate(&display, inner.width),
        usize::from(inner.width),
        Style::default().fg(color(rgb)),
    );
    if focused {
        let cursor = state
            .cursors
            .get(&id)
            .copied()
            .unwrap_or_else(|| input.value.chars().count())
            .min(input.value.chars().count());
        let prefix: String = value.chars().take(cursor).collect();
        let x = inner
            .x
            .saturating_add(u16::try_from(UnicodeWidthStr::width(prefix.as_str())).unwrap_or(0))
            .min(inner.x + inner.width - 1);
        let symbol = value.chars().nth(cursor).unwrap_or(' ');
        buf[(x, inner.y)]
            .set_symbol(&symbol.to_string())
            .set_style(Style::default().fg(color(theme.raised)).bg(color(theme.text)));
    }
}

fn draw_slider<Message: Clone>(
    buf: &mut Buffer,
    area: Rect,
    slider: &Slider<Message>,
    id: WidgetId,
    theme: &Theme,
    state: &mut UiState,
) {
    if slider.on_change.is_some() || slider.on_release.is_some() {
        state.focus_order.push(id);
        state.hits.push(Hit {
            id,
            rect: area,
            kind: HitKind::Slider,
        });
    }
    let area = clamp(area, buf.area);
    if area.width == 0 {
        return;
    }
    let focused = state.focused == Some(id);
    let track = area.width.saturating_sub(1);
    let knob = (slider.fraction() * f64::from(track)).round() as u16;
    for x in 0..=track {
        let symbol = if x < knob { "━" } else { "─" };
        buf[(area.x + x, area.y)]
            .set_symbol(symbol)
            .set_style(Style::default().fg(color(if x <= knob {
                theme.accent
            } else {
                theme.border
            })));
    }
    buf[(area.x + knob, area.y)]
        .set_symbol("●")
        .set_style(Style::default().fg(color(if focused {
            theme.accent
        } else {
            theme.text
        })));
}

fn draw_list<Message: Clone>(
    buf: &mut Buffer,
    area: Rect,
    list: &List<'_, Message>,
    id: WidgetId,
    theme: &Theme,
    state: &mut UiState,
) {
    if list.on_select.is_some() || list.on_activate.is_some() {
        state.focus_order.push(id);
    }
    state.hits.push(Hit {
        id,
        rect: area,
        kind: HitKind::List,
    });
    let row_height = px_to_cells(list.row_height).max(1);
    let visible = usize::from(area.height / row_height);
    if visible == 0 || list.items.is_empty() {
        return;
    }
    let selected = list.selected.filter(|index| *index < list.items.len());
    let mut offset = state.list_offsets.get(&id).copied().unwrap_or(0);
    if let Some(selected) = selected {
        if selected < offset {
            offset = selected;
        } else if selected >= offset + visible {
            offset = selected + 1 - visible;
        }
    }
    offset = offset.min(list.items.len().saturating_sub(visible.min(list.items.len())));
    state.list_offsets.insert(id, offset);

    for (row, index) in (offset..list.items.len()).take(visible).enumerate() {
        let item = &list.items[index];
        let rect = Rect {
            x: area.x,
            y: area.y + row as u16 * row_height,
            width: area.width,
            height: row_height,
        };
        let focused = state.focused == Some(id);
        let (fg, bg) = if item.accent {
            (theme.contrast_on(Tone::Accent), Some(theme.accent))
        } else if selected == Some(index) && focused {
            (theme.text, Some(theme.raised))
        } else {
            (theme.text, None)
        };
        if let Some(bg) = bg {
            fill_rect(buf, rect, bg);
        }
        state.hits.push(Hit {
            id,
            rect,
            kind: HitKind::ListRow(index),
        });
        draw_list_row(buf, rect, item, fg, theme);
    }
}

fn draw_list_row(
    buf: &mut Buffer,
    rect: Rect,
    item: &ferriso::node::ListItem<'_>,
    fg: Rgb,
    theme: &Theme,
) {
    let inner = Rect {
        x: rect.x.saturating_add(1),
        y: rect.y,
        width: rect.width.saturating_sub(2),
        height: rect.height,
    };
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let trailing = item.trailing.as_deref().unwrap_or("");
    let trailing_width = u16::try_from(UnicodeWidthStr::width(trailing)).unwrap_or(0);
    let trailing_width = trailing_width.min(inner.width / 2);
    let left_width = inner.width.saturating_sub(trailing_width + u16::from(!trailing.is_empty()));
    let left = Rect {
        x: inner.x,
        y: inner.y,
        width: left_width,
        height: inner.height,
    };
    let leading = item.leading.as_ref();
    let mut x = left.x;
    if !leading.is_empty() {
        let width = u16::try_from(UnicodeWidthStr::width(leading)).unwrap_or(0);
        buf.set_stringn(
            x,
            left.y,
            truncate(leading, left.width),
            usize::from(left.width),
            Style::default().fg(color(theme.muted)),
        );
        x = x.saturating_add(width + 1).min(left.x + left.width);
    }
    let content_width = left.x.saturating_add(left.width).saturating_sub(x);
    let mut style = Style::default().fg(color(fg));
    if item.accent {
        style = style.add_modifier(Modifier::BOLD);
    }
    buf.set_stringn(
        x,
        left.y,
        truncate(item.title.as_ref(), content_width),
        usize::from(content_width),
        style,
    );
    if let Some(subtitle) = item.subtitle.as_deref() {
        if left.height > 1 {
            buf.set_stringn(
                x,
                left.y + 1,
                truncate(subtitle, content_width),
                usize::from(content_width),
                Style::default().fg(color(theme.muted)),
            );
        } else {
            let used = UnicodeWidthStr::width(truncate(item.title.as_ref(), content_width).as_str());
            let remaining = content_width.saturating_sub(u16::try_from(used).unwrap_or(0));
            if remaining > 3 {
                let text = format!(" · {}", truncate(subtitle, remaining.saturating_sub(3)));
                buf.set_stringn(
                    x + u16::try_from(used).unwrap_or(0),
                    left.y,
                    text,
                    usize::from(remaining),
                    Style::default().fg(color(theme.muted)),
                );
            }
        }
    }
    if !trailing.is_empty() {
        let x = inner
            .x
            .saturating_add(inner.width)
            .saturating_sub(trailing_width);
        buf.set_stringn(
            x,
            inner.y,
            truncate(trailing, trailing_width),
            usize::from(trailing_width),
            Style::default().fg(color(theme.muted)),
        );
    }
}

fn draw_scroll<Message: Clone>(
    buf: &mut Buffer,
    area: Rect,
    scroll: &Scroll<'_, Message>,
    id: WidgetId,
    theme: &Theme,
    state: &mut UiState,
) {
    let area = clamp(area, buf.area);
    if area.width == 0 || area.height == 0 {
        return;
    }
    let padding = scale_padding(scroll.padding);
    let inner = layout::inset(area, padding, false);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    state.scroll_regions.push((id, inner));
    let content_height = layout::intrinsic_height(&scroll.child, inner.width).max(inner.height);
    let max_offset = content_height.saturating_sub(inner.height);
    let offset = state
        .scroll
        .get(&id)
        .copied()
        .unwrap_or(0)
        .min(max_offset);
    state.scroll.insert(id, offset);

    let mut temp = Buffer::empty(Rect::new(0, 0, inner.width, content_height));
    fill_rect(
        &mut temp,
        Rect::new(0, 0, inner.width, content_height),
        theme.background,
    );
    let hit_start = state.hits.len();
    draw_node(
        &mut temp,
        Rect::new(0, 0, inner.width, content_height),
        &scroll.child,
        next_id(id, 0, &scroll.child),
        theme,
        state,
        None,
    );
    let new_hits: Vec<Hit> = state.hits.drain(hit_start..).collect();
    for mut hit in new_hits {
        let shifted = Rect {
            x: inner.x.saturating_add(hit.rect.x),
            y: inner
                .y
                .saturating_add(hit.rect.y)
                .saturating_sub(offset),
            width: hit.rect.width,
            height: hit.rect.height,
        };
        let clipped = clamp(shifted, inner);
        if clipped.width > 0 && clipped.height > 0 {
            hit.rect = clipped;
            state.hits.push(hit);
        }
    }
    for row in 0..inner.height {
        let source = offset + row;
        if source >= content_height {
            break;
        }
        for column in 0..inner.width {
            buf[(inner.x + column, inner.y + row)] = temp[(column, source)].clone();
        }
    }
    if scroll.show_scrollbar && content_height > inner.height && inner.width > 0 {
        let x = inner.x + inner.width - 1;
        let thumb = (u32::from(inner.height) * u32::from(inner.height)
            / u32::from(content_height)) as u16;
        let thumb = thumb.max(1).min(inner.height);
        let travel = inner.height.saturating_sub(thumb);
        let position = if max_offset == 0 {
            0
        } else {
            (u32::from(offset) * u32::from(travel) / u32::from(max_offset)) as u16
        };
        for row in 0..inner.height {
            let symbol = if row >= position && row < position + thumb {
                "┃"
            } else {
                "│"
            };
            buf[(x, inner.y + row)]
                .set_symbol(symbol)
                .set_style(Style::default().fg(color(if row >= position && row < position + thumb {
                    theme.muted
                } else {
                    theme.border
                })));
        }
    }
}

fn draw_image(
    buf: &mut Buffer,
    area: Rect,
    image: &ferriso::node::Image<'_>,
    theme: &Theme,
    forced: Option<Rgb>,
) {
    fill_rect(buf, area, theme.raised);
    let label = if image.alt.is_empty() {
        match image.data {
            Some(ImageData::Encoded { .. }) => "IMG".to_string(),
            Some(ImageData::Rgba { .. }) => "IMG".to_string(),
            None => "♪".to_string(),
        }
    } else {
        image.alt.to_string()
    };
    let rgb = forced.unwrap_or(theme.muted);
    let x = area.x + area.width.saturating_sub(
        u16::try_from(UnicodeWidthStr::width(label.as_str()))
            .unwrap_or(0)
            .min(area.width),
    ) / 2;
    let y = area.y + area.height / 2;
    buf.set_stringn(
        x,
        y,
        truncate(&label, area.width),
        usize::from(area.width),
        Style::default().fg(color(rgb)),
    );
    match image.fit {
        ImageFit::Cover | ImageFit::Contain | ImageFit::Fill => {}
    }
}

fn draw_qr(buf: &mut Buffer, area: Rect, qr: &Qr<'_>, _theme: &Theme) {
    let area = clamp(area, buf.area);
    fill_rect(buf, area, Rgb::WHITE);
    let Ok(code) = QrCode::new(qr.data.as_bytes()) else {
        buf.set_stringn(
            area.x,
            area.y,
            "QR",
            usize::from(area.width),
            Style::default().fg(color(Rgb::BLACK)),
        );
        return;
    };
    let modules = u16::try_from(code.width()).unwrap_or(0);
    let quiet = qr.quiet_zone;
    let total = modules.saturating_add(quiet.saturating_mul(2));
    let output_rows = total.div_ceil(2);
    let origin_x = area.x + area.width.saturating_sub(total) / 2;
    let origin_y = area.y + area.height.saturating_sub(output_rows) / 2;
    let colors = code.to_colors();
    let dark = |row: i32, column: i32| -> bool {
        if row < i32::from(quiet)
            || column < i32::from(quiet)
            || row >= i32::from(quiet + modules)
            || column >= i32::from(quiet + modules)
        {
            return false;
        }
        let row = usize::try_from(row - i32::from(quiet)).unwrap_or(0);
        let column = usize::try_from(column - i32::from(quiet)).unwrap_or(0);
        colors[row * usize::from(modules) + column] == qrcode::Color::Dark
    };
    for output_row in 0..output_rows {
        for column in 0..total {
            let top = dark(i32::from(output_row) * 2, i32::from(column));
            let bottom = dark(i32::from(output_row) * 2 + 1, i32::from(column));
            let (symbol, fg) = match (top, bottom) {
                (true, true) => ("█", Rgb::BLACK),
                (true, false) => ("▀", Rgb::BLACK),
                (false, true) => ("▄", Rgb::BLACK),
                (false, false) => (" ", Rgb::BLACK),
            };
            let x = origin_x + column;
            let y = origin_y + output_row;
            if contains(area, x, y) {
                buf[(x, y)]
                    .set_symbol(symbol)
                    .set_style(Style::default().fg(color(fg)).bg(color(Rgb::WHITE)));
            }
        }
    }
}

fn draw_progress(buf: &mut Buffer, area: Rect, progress: &Progress, theme: &Theme) {
    let area = clamp(area, buf.area);
    fill_rect(buf, area, theme.raised);
    let filled = (f64::from(area.width) * progress.value.clamp(0.0, 1.0)).round() as u16;
    if filled > 0 {
        fill_rect(
            buf,
            Rect {
                x: area.x,
                y: area.y,
                width: filled.min(area.width),
                height: area.height,
            },
            theme.resolve(progress.tone),
        );
    }
}

fn draw_divider(buf: &mut Buffer, area: Rect, divider: &Divider, theme: &Theme) {
    let area = clamp(area, buf.area);
    let style = Style::default().fg(color(theme.resolve(divider.tone)));
    if divider.horizontal {
        for x in area.x..area.x + area.width {
            buf[(x, area.y)].set_symbol("─").set_style(style);
        }
    } else {
        for y in area.y..area.y + area.height {
            buf[(area.x, y)].set_symbol("│").set_style(style);
        }
    }
}

fn clamp(rect: Rect, bounds: Rect) -> Rect {
    rect.intersection(bounds)
}

/// Find a node by its render id. The traversal must mirror [`draw_node`]'s id assignment;
/// both use [`next_id`], so a node found here is the widget the user actually saw.
pub fn find_node<'a, 'v, Message>(
    root: &'v Node<'a, Message>,
    target: WidgetId,
) -> Option<&'v Node<'a, Message>> {
    fn walk<'a, 'v, Message>(
        node: &'v Node<'a, Message>,
        id: WidgetId,
        target: WidgetId,
    ) -> Option<&'v Node<'a, Message>> {
        if id == target {
            return Some(node);
        }
        match node {
            Node::Column(flex) | Node::Row(flex) => {
                for (index, child) in flex.children.iter().enumerate() {
                    if let Some(found) = walk(child, next_id(id, index, child), target) {
                        return Some(found);
                    }
                }
                None
            }
            Node::Button(button) => {
                walk(&button.content, next_id(id, 0, &button.content), target)
            }
            Node::Scroll(scroll) => walk(&scroll.child, next_id(id, 0, &scroll.child), target),
            _ => None,
        }
    }
    walk(root, ROOT_ID, target)
}

pub fn draw<Message: Clone>(
    buf: &mut Buffer,
    area: Rect,
    view: &Node<'_, Message>,
    theme: &Theme,
    state: &mut UiState,
) {
    state.begin_frame();
    fill_rect(buf, area, theme.background);
    draw_node(buf, area, view, ROOT_ID, theme, state, None);
    state.end_frame();
}
