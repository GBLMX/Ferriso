//! Terminal input: focus traversal, text editing, sliders, lists, and mouse hit-testing.

use crossterm::event::{
    KeyCode, KeyEvent as CrosstermKey, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use ferriso::node::{List, Node, Slider, TextInput};
use ferriso::{App, Command, Key, KeyEvent, Modifiers, WidgetId};

use crate::layout::scale_padding;
use crate::render::{self, HitKind, UiState};

pub fn map_key(event: CrosstermKey) -> KeyEvent {
    let key = match event.code {
        KeyCode::Char(' ') => Key::Space,
        KeyCode::Char(ch) => Key::Character(ch.to_string()),
        KeyCode::Enter => Key::Enter,
        KeyCode::Tab => Key::Tab,
        KeyCode::BackTab => Key::BackTab,
        KeyCode::Esc => Key::Escape,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Delete => Key::Delete,
        KeyCode::Insert => Key::Insert,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::F(number) => Key::Function(number),
        _ => Key::Other,
    };
    let text = match &key {
        Key::Character(ch) => Some(ch.clone()),
        Key::Space => Some(" ".to_string()),
        _ => None,
    };
    KeyEvent {
        key,
        modifiers: Modifiers {
            control: event.modifiers.contains(KeyModifiers::CONTROL),
            shift: event.modifiers.contains(KeyModifiers::SHIFT),
            alt: event.modifiers.contains(KeyModifiers::ALT),
            logo: event.modifiers.contains(KeyModifiers::SUPER),
        },
        pressed: event.kind != KeyEventKind::Release,
        repeat: event.kind == KeyEventKind::Repeat,
        text,
    }
}

pub fn handle_key<A: App>(
    app: &mut A,
    state: &mut UiState,
    event: KeyEvent,
) -> Command<A::Message> {
    if !event.pressed {
        return app.on_key(event);
    }
    if event.modifiers.control
        && matches!(&event.key, Key::Character(ch) if ch.eq_ignore_ascii_case("c"))
    {
        state.quit = true;
        return Command::none();
    }
    if event.modifiers.is_shortcut() {
        return app.on_key(event);
    }
    match event.key {
        Key::Tab => {
            move_focus(state, 1);
            return Command::none();
        }
        Key::BackTab => {
            move_focus(state, -1);
            return Command::none();
        }
        _ => {}
    }

    let Some(focused) = state.focused else {
        return app.on_key(event);
    };
    let message = {
        let view = app.view();
        render::find_node(&view, focused).and_then(|node| resolve_key(node, focused, &event, state))
    };
    match message {
        Some(message) => app.update(message),
        None => app.on_key(event),
    }
}

fn move_focus(state: &mut UiState, delta: i32) {
    if state.focus_order.is_empty() {
        state.focused = None;
        return;
    }
    let current = state
        .focused
        .and_then(|focused| state.focus_order.iter().position(|id| *id == focused));
    let next = match current {
        Some(index) => {
            let len = state.focus_order.len() as i32;
            (index as i32 + delta).rem_euclid(len) as usize
        }
        None if delta >= 0 => 0,
        None => state.focus_order.len() - 1,
    };
    state.focused = Some(state.focus_order[next]);
}

fn resolve_key<Message: Clone>(
    node: &Node<'_, Message>,
    id: WidgetId,
    event: &KeyEvent,
    state: &mut UiState,
) -> Option<Message> {
    match node {
        Node::Button(button) => {
            if matches!(event.key, Key::Enter | Key::Space) {
                button.on_press.clone()
            } else {
                None
            }
        }
        Node::TextInput(input) => edit_input(input, id, event, state),
        Node::Slider(slider) => slide(slider, event),
        Node::List(list) => list_key(list, event),
        _ => None,
    }
}

fn edit_input<Message: Clone>(
    input: &TextInput<'_, Message>,
    id: WidgetId,
    event: &KeyEvent,
    state: &mut UiState,
) -> Option<Message> {
    let on_input = input.on_input.as_ref()?;
    let value = input.value.to_string();
    let chars: Vec<char> = value.chars().collect();
    let mut cursor = state
        .cursors
        .get(&id)
        .copied()
        .unwrap_or(chars.len())
        .min(chars.len());

    let new_value = match &event.key {
        Key::Enter => return input.on_submit.clone(),
        Key::Backspace => {
            if cursor == 0 {
                return None;
            }
            cursor -= 1;
            let mut next = chars.clone();
            next.remove(cursor);
            next.into_iter().collect()
        }
        Key::Delete => {
            if cursor >= chars.len() {
                return None;
            }
            let mut next = chars.clone();
            next.remove(cursor);
            next.into_iter().collect()
        }
        Key::Left => {
            cursor = cursor.saturating_sub(1);
            state.cursors.insert(id, cursor);
            return None;
        }
        Key::Right => {
            cursor = (cursor + 1).min(chars.len());
            state.cursors.insert(id, cursor);
            return None;
        }
        Key::Home => {
            state.cursors.insert(id, 0);
            return None;
        }
        Key::End => {
            state.cursors.insert(id, chars.len());
            return None;
        }
        _ => {
            let text = event.text()?;
            if text.chars().any(char::is_control) {
                return None;
            }
            let mut next = chars.clone();
            for (offset, ch) in text.chars().enumerate() {
                next.insert(cursor + offset, ch);
            }
            cursor += text.chars().count();
            next.into_iter().collect()
        }
    };
    state.cursors.insert(id, cursor);
    Some(on_input.call(new_value))
}

fn slide<Message: Clone>(slider: &Slider<Message>, event: &KeyEvent) -> Option<Message> {
    let span = (slider.max - slider.min).max(f64::EPSILON);
    let step = slider.step.unwrap_or(span / 100.0).max(f64::EPSILON);
    let value = match event.key {
        Key::Left | Key::Down => (slider.value - step).max(slider.min),
        Key::Right | Key::Up => (slider.value + step).min(slider.max),
        Key::Home => slider.min,
        Key::End => slider.max,
        Key::Enter | Key::Space => return slider.on_release.clone(),
        _ => return None,
    };
    slider.on_change.as_ref().map(|run| run.call(value))
}

fn list_key<Message: Clone>(list: &List<'_, Message>, event: &KeyEvent) -> Option<Message> {
    if list.items.is_empty() {
        return None;
    }
    let last = list.items.len() - 1;
    let selected = list.selected.unwrap_or(0).min(last);
    let next = match event.key {
        Key::Up => selected.saturating_sub(1),
        Key::Down => (selected + 1).min(last),
        Key::Home => 0,
        Key::End => last,
        Key::PageUp => selected.saturating_sub(10),
        Key::PageDown => (selected + 10).min(last),
        Key::Enter | Key::Space => {
            return list
                .on_activate
                .as_ref()
                .or(list.on_select.as_ref())
                .map(|run| run.call(selected));
        }
        _ => return None,
    };
    if Some(next) == list.selected {
        return None;
    }
    list.on_select.as_ref().map(|run| run.call(next))
}

pub fn handle_mouse<A: App>(
    app: &mut A,
    state: &mut UiState,
    event: MouseEvent,
) -> Command<A::Message> {
    let position = (event.column, event.row);
    match event.kind {
        MouseEventKind::Moved => {
            state.mouse = Some(position);
            return Command::none();
        }
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
            if let Some(id) = state.scroll_at(event.column, event.row) {
                let offset = state.scroll.get(&id).copied().unwrap_or(0);
                let delta = if event.kind == MouseEventKind::ScrollUp {
                    -3
                } else {
                    3
                };
                state.scroll.insert(id, offset.saturating_add_signed(delta));
            }
            return Command::none();
        }
        MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Drag(MouseButton::Left) => {
            state.mouse = Some(position);
            let Some(hit) = state.hit_at(event.column, event.row).cloned() else {
                if event.kind == MouseEventKind::Down(MouseButton::Left) {
                    state.focused = None;
                }
                return Command::none();
            };
            let clicked = event.kind == MouseEventKind::Down(MouseButton::Left);
            if clicked {
                state.focused = Some(hit.id);
            }
            let message = {
                let view = app.view();
                let node = render::find_node(&view, hit.id);
                node.and_then(|node| match (node, hit.kind) {
                    (Node::Button(button), HitKind::Button) => button.on_press.clone(),
                    (Node::List(list), HitKind::ListRow(index)) => {
                        if clicked {
                            list.on_activate
                                .as_ref()
                                .or(list.on_select.as_ref())
                                .map(|run| run.call(index))
                        } else {
                            None
                        }
                    }
                    (Node::TextInput(input), HitKind::TextInput) => {
                        if clicked {
                            let cursor = input
                                .value
                                .chars()
                                .count()
                                .min(usize::from(u16::MAX));
                            // Place the caret near the click, not always at the end.
                            let inner_x = hit
                                .rect
                                .x
                                .saturating_add(scale_padding(input.padding).left)
                                .saturating_add(1);
                            let target = event.column.saturating_sub(inner_x);
                            let mut width = 0u16;
                            let mut index = 0usize;
                            for ch in input.value.chars() {
                                let step = u16::try_from(
                                    unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0),
                                )
                                .unwrap_or(0);
                                if width + step / 2 > target {
                                    break;
                                }
                                width += step;
                                index += 1;
                            }
                            state.cursors.insert(hit.id, index.min(cursor));
                        }
                        None
                    }
                    (Node::Slider(slider), HitKind::Slider) => {
                        let width = hit.rect.width.saturating_sub(1).max(1);
                        let fraction = f64::from(
                            event.column.saturating_sub(hit.rect.x).min(width),
                        ) / f64::from(width);
                        let value =
                            slider.min + fraction.clamp(0.0, 1.0) * (slider.max - slider.min);
                        state.slider_drag = Some(hit.id);
                        slider.on_change.as_ref().map(|run| run.call(value))
                    }
                    _ => None,
                })
            };
            message.map_or_else(Command::none, |message| app.update(message))
        }
        MouseEventKind::Up(MouseButton::Left) => {
            let Some(id) = state.slider_drag.take() else {
                return Command::none();
            };
            let release = {
                let view = app.view();
                match render::find_node(&view, id) {
                    Some(Node::Slider(slider)) => slider.on_release.clone(),
                    _ => None,
                }
            };
            release.map_or_else(Command::none, |message| app.update(message))
        }
        _ => Command::none(),
    }
}
