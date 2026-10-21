//! Terminal layout arithmetic.
//!
//! Integer cells, no browser-style measurement: intrinsic sizes come from text width and the
//! node tree. A GUI backend delegates this to its layout engine, so this module is deliberately
//! only used by the terminal renderer.

use ferriso::node::{Align, Length, Node};
use ferriso::node::{Padding, Qr};
use ratatui::layout::Rect;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Logical units are pixels in the view tree. A terminal cell is treated as an 8 px square so
/// that the same `Fixed(190)` sidebar is 24 columns here and 190 px in a window.
pub const CELL_PX: u16 = 8;

pub fn px_to_cells(px: u16) -> u16 {
    if px == 0 {
        0
    } else {
        px.saturating_add(CELL_PX / 2).saturating_div(CELL_PX).max(1)
    }
}

pub fn scale_length(length: Length) -> Length {
    match length {
        Length::Fixed(px) => Length::Fixed(px_to_cells(px)),
        other => other,
    }
}

pub fn scale_padding(padding: Padding) -> Padding {
    Padding::new(
        px_to_cells(padding.top),
        px_to_cells(padding.right),
        px_to_cells(padding.bottom),
        px_to_cells(padding.left),
    )
}

/// Inner area after padding and the one-cell border.
pub fn inset(area: Rect, padding: Padding, border: bool) -> Rect {
    let border = u16::from(border);
    let left = padding.left.saturating_add(border);
    let right = padding.right.saturating_add(border);
    let top = padding.top.saturating_add(border);
    let bottom = padding.bottom.saturating_add(border);
    Rect {
        x: area.x.saturating_add(left),
        y: area.y.saturating_add(top),
        width: area.width.saturating_sub(left).saturating_sub(right),
        height: area.height.saturating_sub(top).saturating_sub(bottom),
    }
}

/// Split `total` cells between children. Gaps are reserved first; fixed and intrinsic sizes are
/// honoured when they fit, and the remainder goes to `Fill`/`FillPortion` siblings. When there
/// is no fill child the leftover is given to the last child, which is what a terminal user
/// expects from a header plus body.
pub fn split(total: u16, gap: u16, items: &[(Length, u16)]) -> Vec<u16> {
    if items.is_empty() {
        return Vec::new();
    }
    let gaps = gap.saturating_mul(items.len().saturating_sub(1) as u16);
    let available = total.saturating_sub(gaps);

    let mut sizes = vec![0u16; items.len()];
    let mut fixed_total = 0u16;
    let mut weight_total = 0u32;
    for (index, (length, intrinsic)) in items.iter().enumerate() {
        match length {
            Length::Fixed(value) => {
                sizes[index] = *value;
                fixed_total = fixed_total.saturating_add(*value);
            }
            Length::Shrink => {
                sizes[index] = *intrinsic;
                fixed_total = fixed_total.saturating_add(*intrinsic);
            }
            Length::Fill => weight_total += 1,
            Length::FillPortion(weight) => weight_total += u32::from((*weight).max(1)),
        }
    }

    if fixed_total > available {
        // Not enough room: drop the tail. Widgets then clip rather than spill into siblings.
        let mut remaining = available;
        for size in &mut sizes {
            let take = (*size).min(remaining);
            *size = take;
            remaining -= take;
        }
        return sizes;
    }

    let remaining = available - fixed_total;
    if weight_total > 0 {
        let mut used = 0u16;
        for (index, (length, _)) in items.iter().enumerate() {
            let weight = match length {
                Length::Fill => 1,
                Length::FillPortion(weight) => u32::from((*weight).max(1)),
                _ => 0,
            };
            if weight == 0 {
                continue;
            }
            let share = (u32::from(remaining) * weight / weight_total) as u16;
            sizes[index] = share;
            used = used.saturating_add(share);
        }
        // Integer division leaves a few cells; give them to the last fill child.
        if used < remaining {
            if let Some((index, _)) = items.iter().enumerate().rev().find(|(_, (length, _))| {
                matches!(length, Length::Fill | Length::FillPortion(_))
            }) {
                sizes[index] = sizes[index].saturating_add(remaining - used);
            }
        }
    } else if let Some(last) = sizes.last_mut() {
        *last = last.saturating_add(remaining);
    }

    sizes
}

/// Offset a rect inside a cross-axis area according to its alignment.
pub fn align_offset(container: u16, child: u16, align: Align) -> u16 {
    let spare = container.saturating_sub(child);
    match align {
        Align::Start => 0,
        Align::Center => spare / 2,
        Align::End => spare,
    }
}

pub fn text_width(text: &str) -> u16 {
    text.lines()
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0)
        .min(usize::from(u16::MAX)) as u16
}

/// Wrap into display lines. Spaces are kept on the line they end; a word longer than the width
/// is split by character (CJK text is routinely one long run).
pub fn wrap_text(text: &str, width: u16, wrap: bool) -> Vec<String> {
    let width = usize::from(width.max(1));
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        if !wrap {
            lines.push(paragraph.to_string());
            continue;
        }
        let mut line = String::new();
        let mut line_width = 0usize;
        for word in paragraph.split_inclusive(' ') {
            let word_width = UnicodeWidthStr::width(word);
            if line_width + word_width <= width {
                line.push_str(word);
                line_width += word_width;
                continue;
            }
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
                line_width = 0;
            }
            if word_width <= width {
                let trimmed = word.trim_start_matches(' ');
                line.push_str(trimmed);
                line_width = UnicodeWidthStr::width(trimmed);
                continue;
            }
            for ch in word.chars() {
                let ch_width = ch.width().unwrap_or(0);
                if line_width + ch_width > width && !line.is_empty() {
                    lines.push(std::mem::take(&mut line));
                    line_width = 0;
                }
                if ch != ' ' || !line.is_empty() {
                    line.push(ch);
                    line_width += ch_width;
                }
            }
        }
        lines.push(line);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

pub fn text_height(text: &str, width: u16, wrap: bool) -> u16 {
    wrap_text(text, width, wrap).len().min(usize::from(u16::MAX)) as u16
}

fn children_height<Message>(children: &[Node<'_, Message>], width: u16, spacing: u16) -> u16 {
    let mut height = 0u16;
    for (index, child) in children.iter().enumerate() {
        if index > 0 {
            height = height.saturating_add(spacing);
        }
        height = height.saturating_add(intrinsic_height(child, width));
    }
    height
}

fn children_width<Message>(children: &[Node<'_, Message>], height: u16, spacing: u16) -> u16 {
    let mut width = 0u16;
    for (index, child) in children.iter().enumerate() {
        if index > 0 {
            width = width.saturating_add(spacing);
        }
        width = width.saturating_add(intrinsic_width(child, height));
    }
    width
}

/// Height a node wants at `width` cells. The result is a hint, not a promise: drawing clamps to
/// the real area so a layout mistake degrades into clipping instead of an overflow.
pub fn intrinsic_height<Message>(node: &Node<'_, Message>, width: u16) -> u16 {
    match node {
        Node::Column(flex) => {
            let padding = scale_padding(flex.padding);
            let outer_width = match scale_length(flex.width) {
                Length::Fixed(value) => value,
                _ => width,
            };
            let content_width = outer_width.saturating_sub(padding.horizontal());
            let content_width = if flex.surface.border.is_some() {
                content_width.saturating_sub(2)
            } else {
                content_width
            };
            padding
                .vertical()
                .saturating_add(u16::from(flex.surface.border.is_some()) * 2)
                .saturating_add(children_height(
                    &flex.children,
                    content_width,
                    px_to_cells(flex.spacing),
                ))
        }
        Node::Row(flex) => {
            let padding = scale_padding(flex.padding);
            let content_width = width
                .saturating_sub(padding.horizontal())
                .saturating_sub(u16::from(flex.surface.border.is_some()) * 2);
            let mut tallest = 0u16;
            for child in &flex.children {
                tallest = tallest.max(intrinsic_height(child, content_width));
            }
            padding
                .vertical()
                .saturating_add(u16::from(flex.surface.border.is_some()) * 2)
                .saturating_add(tallest)
        }
        Node::Text(text) => {
            let available = match scale_length(text.style.width) {
                Length::Fixed(value) => value,
                Length::Shrink => text_width(&text.content).min(width.max(1)),
                _ => width,
            };
            text_height(&text.content, available, text.style.wrap)
        }
        Node::Button(button) => {
            let padding = scale_padding(button.padding);
            intrinsic_height(&button.content, width)
                .saturating_add(padding.vertical())
                .max(1)
        }
        Node::TextInput(input) => 3u16
            .saturating_add(scale_padding(input.padding).vertical()),
        Node::Slider(_) => 1,
        Node::List(list) => match scale_length(list.height) {
            Length::Fixed(value) => value,
            _ => (list.items.len() as u16).saturating_mul(px_to_cells(list.row_height).max(1)),
        },
        Node::Scroll(scroll) => match scale_length(scroll.height) {
            Length::Fixed(value) => value,
            _ => {
                let padding = scale_padding(scroll.padding);
                intrinsic_height(&scroll.child, width.saturating_sub(padding.horizontal()))
                    .saturating_add(padding.vertical())
            }
        },
        Node::Image(image) => match scale_length(image.height) {
            Length::Fixed(value) => value,
            Length::Shrink => match scale_length(image.width) {
                Length::Fixed(value) => value,
                _ => 3,
            },
            _ => width,
        },
        Node::Qr(qr) => qr_size(qr).1,
        Node::Progress(_) | Node::Divider(_) => 1,
        Node::Space(space) => match scale_length(space.height) {
            Length::Fixed(value) => value,
            _ => 1,
        },
    }
}

/// Width a node wants at `height` cells.
pub fn intrinsic_width<Message>(node: &Node<'_, Message>, height: u16) -> u16 {
    match node {
        Node::Column(flex) => match scale_length(flex.width) {
            Length::Fixed(value) => value,
            _ => {
                let padding = scale_padding(flex.padding);
                flex.children
                    .iter()
                    .map(|child| intrinsic_width(child, height))
                    .max()
                    .unwrap_or(0)
                    .saturating_add(padding.horizontal())
                    .saturating_add(u16::from(flex.surface.border.is_some()) * 2)
            }
        },
        Node::Row(flex) => match scale_length(flex.width) {
            Length::Fixed(value) => value,
            Length::Shrink => {
                let padding = scale_padding(flex.padding);
                children_width(&flex.children, height, px_to_cells(flex.spacing))
                    .saturating_add(padding.horizontal())
                    .saturating_add(u16::from(flex.surface.border.is_some()) * 2)
            }
            _ => u16::MAX / 4,
        },
        Node::Text(text) => match scale_length(text.style.width) {
            Length::Fixed(value) => value,
            Length::Fill | Length::FillPortion(_) => u16::MAX / 4,
            Length::Shrink => text_width(&text.content),
        },
        Node::Button(button) => {
            let padding = scale_padding(button.padding);
            intrinsic_width(&button.content, height)
                .saturating_add(padding.horizontal())
                .max(1)
        }
        Node::TextInput(input) => match scale_length(input.width) {
            Length::Fixed(value) => value,
            _ => u16::MAX / 4,
        },
        Node::Slider(slider) => match scale_length(slider.width) {
            Length::Fixed(value) => value,
            _ => u16::MAX / 4,
        },
        Node::List(list) => match scale_length(list.width) {
            Length::Fixed(value) => value,
            _ => u16::MAX / 4,
        },
        Node::Scroll(scroll) => match scale_length(scroll.width) {
            Length::Fixed(value) => value,
            _ => u16::MAX / 4,
        },
        Node::Image(image) => match scale_length(image.width) {
            Length::Fixed(value) => value,
            _ => match scale_length(image.height) {
                Length::Fixed(value) => value,
                _ => 3,
            },
        },
        Node::Qr(qr) => qr_size(qr).0,
        Node::Progress(_) | Node::Divider(_) => 1,
        Node::Space(space) => match scale_length(space.width) {
            Length::Fixed(value) => value,
            _ => 1,
        },
    }
}

/// QR codes are drawn two module rows per terminal row (half blocks), so the height is half
/// the width and the code keeps its square proportions on a character cell.
pub fn qr_size(qr: &Qr<'_>) -> (u16, u16) {
    let Ok(code) = qrcode::QrCode::new(qr.data.as_bytes()) else {
        return (3, 2);
    };
    let modules = code.width() as u16;
    let total = modules.saturating_add(qr.quiet_zone.saturating_mul(2));
    (total, total.div_ceil(2))
}

/// Width-aware truncation with an ellipsis, used by list rows and status lines.
pub fn truncate(text: &str, width: u16) -> String {
    let width = usize::from(width);
    if UnicodeWidthStr::width(text) <= width {
        return text.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut used = 0usize;
    for ch in text.chars() {
        let ch_width = ch.width().unwrap_or(0);
        if used + ch_width > width.saturating_sub(1) {
            break;
        }
        out.push(ch);
        used += ch_width;
    }
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_share_the_remainder_and_fixed_children_keep_their_size() {
        assert_eq!(
            split(20, 1, &[(Length::Fixed(5), 0), (Length::Fill, 0)]),
            vec![5, 14]
        );
        assert_eq!(
            split(20, 1, &[(Length::Fill, 0), (Length::Fill, 0)]),
            vec![9, 10]
        );
        assert_eq!(
            split(10, 0, &[(Length::FillPortion(1), 0), (Length::FillPortion(2), 0)]),
            vec![3, 7]
        );
    }

    #[test]
    fn wrapping_counts_display_width_not_bytes() {
        assert_eq!(wrap_text("abcdef", 3, true), vec!["abc", "def"]);
        assert_eq!(wrap_text("日本語", 4, true), vec!["日本", "語"]);
        assert_eq!(text_width("日本語"), 6);
    }

    #[test]
    fn truncation_keeps_the_visible_prefix() {
        assert_eq!(truncate("abcdef", 4), "abc…");
        assert_eq!(truncate("abc", 4), "abc");
        assert_eq!(truncate("日本語", 4), "日…");
    }

    #[test]
    fn no_child_overflows_a_short_parent() {
        let sizes = split(4, 0, &[(Length::Fixed(3), 0), (Length::Fixed(3), 0)]);
        assert_eq!(sizes.iter().sum::<u16>(), 4);
    }
}
