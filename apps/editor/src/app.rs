//! iced shell for the actor-owned editor.
//!
//! All domain state lives in [`EditorModel`](crate::model::EditorModel). This
//! module owns only toolkit-local state: preedit text, window size, resize
//! debounce, and focus bookkeeping.

use std::time::{Duration, Instant};

use ferriso_gui::{
    Renderer,
    iced::{
        self, Alignment, Element, Font, Length, Point, Rectangle, Size, Subscription, Task, Theme,
        keyboard::{self, Key, key::Named},
        widget::{
            Row, Space, button, column, container, opaque, operation, row, scrollable, stack, text,
            text_input,
        },
        window,
    },
};

use crate::{
    canvas::{CELL_WIDTH, EditorCanvas, ImeCanvas, LINE_HEIGHT, TEXT_LEFT, TEXT_TOP},
    model::{EditorMessage, EditorSnapshot, FocusTarget, Modal},
    theme,
};

const HEADER_HEIGHT: f32 = 56.0;
const TABS_HEIGHT: f32 = 38.0;
const STATUS_HEIGHT: f32 = 30.0;
const SEARCH_HEIGHT: f32 = 56.0;
const REPLACE_HEIGHT: f32 = 96.0;

pub type Message = ferriso_gui::Message<EditorSnapshot, EditorMessage, EditorLocal>;

#[derive(Clone, Debug)]
pub enum EditorLocal {
    Resize(Size),
    Rescaled(window::Id),
    ResizeSettled,
    ImeOpened,
    ImePreedit(String),
    ImeCommit(String),
    ImeClosed,
}

pub struct EditorRenderer {
    preedit: Option<String>,
    window_size: Size,
    last_viewport: Option<(usize, usize)>,
    resize_deadline: Option<Instant>,
    last_focus: Option<u64>,
}

impl EditorRenderer {
    pub fn new(size: (f32, f32)) -> Self {
        Self {
            preedit: None,
            window_size: Size::new(size.0, size.1),
            last_viewport: None,
            resize_deadline: None,
            last_focus: None,
        }
    }

    fn query_id() -> iced::widget::Id {
        iced::widget::Id::new("ferriso.find.query")
    }

    fn replacement_id() -> iced::widget::Id {
        iced::widget::Id::new("ferriso.find.replacement")
    }

    fn layout(&self, view: &EditorSnapshot) -> (usize, usize) {
        let search_height = if view.search.open {
            if view.search.replace {
                REPLACE_HEIGHT
            } else {
                SEARCH_HEIGHT
            }
        } else {
            0.0
        };
        let rows = ((self.window_size.height
            - HEADER_HEIGHT
            - TABS_HEIGHT
            - STATUS_HEIGHT
            - search_height)
            / LINE_HEIGHT)
            .floor()
            .max(1.0) as usize;
        let columns = ((self.window_size.width - TEXT_LEFT - 28.0) / CELL_WIDTH)
            .floor()
            .max(10.0) as usize;
        (rows, columns)
    }

    fn viewport_task(&mut self, view: &EditorSnapshot) -> Task<Message> {
        let viewport = self.layout(view);
        if self.last_viewport == Some(viewport) {
            return Task::none();
        }
        self.last_viewport = Some(viewport);
        Task::done(Message::User(EditorMessage::Viewport {
            rows: viewport.0,
            columns: viewport.1,
        }))
    }

    fn header(&self, view: &EditorSnapshot) -> Element<'_, Message> {
        let brand = row![
            container(Space::new().width(3).height(18))
                .style(|_| container::Style {
                    background: Some(iced::Background::Color(theme::ACCENT)),
                    ..Default::default()
                })
                .padding(0),
            text("FERRISO")
                .size(15)
                .font(Font::MONOSPACE)
                .color(theme::TEXT),
            text("EDITOR")
                .size(10)
                .font(Font::MONOSPACE)
                .color(theme::DIM),
        ]
        .spacing(7)
        .align_y(Alignment::Center);

        let actions = row![
            action_button("NEW", EditorMessage::New, false),
            action_button("OPEN", EditorMessage::Open, false),
            action_button("SAVE", EditorMessage::Save, false),
            action_button("UNDO", EditorMessage::Undo, false),
            action_button("REDO", EditorMessage::Redo, false),
            action_button("FIND", EditorMessage::Find, view.search.open),
            action_button("WRAP", EditorMessage::ToggleWrap, view.view.wrap),
        ]
        .spacing(4)
        .align_y(Alignment::Center);

        container(
            row![
                brand,
                Space::new().width(Length::Fixed(22.0)),
                actions,
                Space::new().width(Length::Fill),
                text(truncate(view.active_name(), 38))
                    .size(11)
                    .font(Font::MONOSPACE)
                    .color(theme::MUTED),
            ]
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .height(Length::Fixed(HEADER_HEIGHT))
        .padding([0, 16])
        .style(theme::toolbar)
        .into()
    }

    fn tab_strip(&self, view: &EditorSnapshot) -> Element<'_, Message> {
        let mut children: Vec<Element<'_, Message>> = Vec::with_capacity(view.tabs.len());
        for (index, tab) in view.tabs.iter().enumerate() {
            let active = index == view.active;
            let select = button(
                row![
                    text(if tab.dirty { "●" } else { " " })
                        .size(9)
                        .color(if tab.dirty { theme::AMBER } else { theme::DIM }),
                    text(truncate(&tab.name, 24))
                        .size(11)
                        .font(Font::MONOSPACE)
                        .color(if active { theme::TEXT } else { theme::MUTED }),
                ]
                .spacing(4)
                .align_y(Alignment::Center),
            )
            .padding([4, 8])
            .style(theme::ghost)
            .on_press(Message::User(EditorMessage::SelectTab(index)));

            let close = button(text("×").size(13).color(theme::DIM))
                .padding([2, 5])
                .style(theme::ghost)
                .on_press(Message::User(EditorMessage::CloseTab(index)));

            children.push(
                container(row![select, close].spacing(2).align_y(Alignment::Center))
                    .style(theme::tab(active))
                    .padding([1, 2])
                    .into(),
            );
        }

        let tabs = Row::with_children(children)
            .spacing(5)
            .align_y(Alignment::Center);
        let strip = scrollable(tabs).horizontal().width(Length::Fill);
        let bar = row![
            strip,
            action_button("＋", EditorMessage::New, false),
            Space::new().width(Length::Fixed(4.0)),
        ]
        .spacing(6)
        .align_y(Alignment::Center);

        container(bar)
            .width(Length::Fill)
            .height(Length::Fixed(TABS_HEIGHT))
            .padding([0, 12])
            .style(theme::panel)
            .into()
    }

    fn search_panel(&self, view: &EditorSnapshot) -> Element<'_, Message> {
        let search = &view.search;
        let query = text_input("find in document…", &search.query)
            .id(Self::query_id())
            .on_input(|value| Message::User(EditorMessage::SearchQuery(value)))
            .on_submit(Message::User(EditorMessage::FindNext))
            .padding([6, 9])
            .size(12)
            .width(Length::Fixed(290.0))
            .style(theme::search_input);

        let count = if search.query.is_empty() {
            String::new()
        } else if search.matches.is_empty() {
            "no matches".to_string()
        } else {
            search.current.map_or_else(
                || format!("{} matches", search.matches.len()),
                |index| format!("{}/{}", index + 1, search.matches.len()),
            )
        };

        let first_row = row![
            text("FIND").size(9).font(Font::MONOSPACE).color(theme::DIM),
            query,
            action_button("Aa", EditorMessage::ToggleCase, search.case_sensitive),
            action_button("‹", EditorMessage::FindPrevious, false),
            action_button("›", EditorMessage::FindNext, false),
            text(count)
                .size(10)
                .font(Font::MONOSPACE)
                .color(theme::MUTED),
            Space::new().width(Length::Fill),
            action_button("×", EditorMessage::CloseSearch, false),
        ]
        .spacing(6)
        .align_y(Alignment::Center);

        let mut content = column![first_row].spacing(6);
        if search.replace {
            let replacement = text_input("replace with…", &search.replacement)
                .id(Self::replacement_id())
                .on_input(|value| Message::User(EditorMessage::Replacement(value)))
                .on_submit(Message::User(EditorMessage::ReplaceCurrent))
                .padding([6, 9])
                .size(12)
                .width(Length::Fixed(290.0))
                .style(theme::search_input);
            let second_row = row![
                text("WITH").size(9).font(Font::MONOSPACE).color(theme::DIM),
                replacement,
                action_button("REPLACE", EditorMessage::ReplaceCurrent, false),
                action_button("ALL", EditorMessage::ReplaceAll, false),
            ]
            .spacing(6)
            .align_y(Alignment::Center);
            content = content.push(second_row);
        }

        let panel = container(content)
            .style(theme::search_panel)
            .padding([8, 10])
            .width(Length::Fixed(720.0));

        container(panel)
            .width(Length::Fill)
            .padding([6, 12])
            .align_x(Alignment::Center)
            .style(theme::panel)
            .into()
    }

    fn body<'a>(&'a self, view: &'a EditorSnapshot) -> Element<'a, Message> {
        let canvas = ImeCanvas::new(
            EditorCanvas {
                view: &view.view,
                preedit: self.preedit.as_deref(),
            },
            self.ime_cursor(view),
            self.ime_active(view),
        );
        let editor: Element<'_, Message> = container(canvas).style(theme::editor).into();

        if is_welcome(view) {
            let welcome = container(
                column![
                    text("FERRISO")
                        .size(42)
                        .font(Font::MONOSPACE)
                        .color(theme::ACCENT),
                    text("a calm place for text").size(13).color(theme::MUTED),
                    Space::new().height(Length::Fixed(18.0)),
                    row![
                        button(
                            text("OPEN A FILE")
                                .size(10)
                                .font(Font::MONOSPACE)
                                .color(theme::BG),
                        )
                        .padding([9, 14])
                        .style(theme::primary)
                        .on_press(Message::User(EditorMessage::Open)),
                        button(
                            text("NEW BUFFER")
                                .size(10)
                                .font(Font::MONOSPACE)
                                .color(theme::TEXT),
                        )
                        .padding([9, 14])
                        .style(theme::subtle)
                        .on_press(Message::User(EditorMessage::New)),
                    ]
                    .spacing(8),
                    Space::new().height(Length::Fixed(22.0)),
                    text("Ctrl+O  open      Ctrl+N  new tab      Ctrl+F  find")
                        .size(10)
                        .font(Font::MONOSPACE)
                        .color(theme::DIM),
                ]
                .spacing(6)
                .align_x(Alignment::Center),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center);
            stack(vec![editor, welcome.into()])
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        } else {
            editor
        }
    }

    fn status_bar(&self, view: &EditorSnapshot) -> Element<'_, Message> {
        let editor_view = &view.view;
        let path = editor_view
            .path
            .as_ref()
            .map_or_else(|| "untitled".to_string(), |path| path.display().to_string());
        let ending = match editor_view.line_ending {
            ferriso_editor::LineEnding::Lf => "LF",
            ferriso_editor::LineEnding::CrLf => "CRLF",
        };
        let language = editor_view.language.unwrap_or("plain");
        let left = row![
            text("●").size(9).color(if editor_view.dirty {
                theme::AMBER
            } else {
                theme::GREEN
            }),
            text(truncate(&path, 56))
                .size(10)
                .font(Font::MONOSPACE)
                .color(theme::MUTED),
            text(language)
                .size(9)
                .font(Font::MONOSPACE)
                .color(theme::ACCENT),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let status = if editor_view.status.is_empty() {
            format!(
                "Ln {}, Col {}   ·   {} {}   ·   {}   ·   UTF-8   ·   {}",
                editor_view.cursor_line,
                editor_view.cursor_column,
                editor_view.line_count,
                if editor_view.line_count == 1 {
                    "line"
                } else {
                    "lines"
                },
                if editor_view.wrap { "wrap" } else { "no wrap" },
                ending
            )
        } else {
            editor_view.status.clone()
        };

        container(
            row![
                left,
                Space::new().width(Length::Fill),
                text(status).size(10).font(Font::MONOSPACE).color(
                    if editor_view.status.is_empty() {
                        theme::MUTED
                    } else {
                        theme::AMBER
                    }
                ),
                text(if editor_view.dirty {
                    "UNSAVED"
                } else {
                    "SAVED"
                })
                .size(9)
                .font(Font::MONOSPACE)
                .color(if editor_view.dirty {
                    theme::AMBER
                } else {
                    theme::GREEN
                }),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .height(Length::Fixed(STATUS_HEIGHT))
        .padding([0, 14])
        .style(theme::toolbar)
        .into()
    }

    fn modal_view(&self, view: &EditorSnapshot) -> Option<Element<'_, Message>> {
        let modal = view.modal?;
        let (title, body, confirm) = match modal {
            Modal::CloseTab(index) => {
                let name = view
                    .tabs
                    .get(index)
                    .map_or_else(|| "this file".to_string(), |tab| tab.name.clone());
                (
                    format!("Close “{}”?", truncate(&name, 34)),
                    "This tab has unsaved changes.".to_string(),
                    "SAVE & CLOSE",
                )
            }
            Modal::Quit => {
                let dirty = view.dirty_count;
                (
                    "Quit Ferriso?".to_string(),
                    format!(
                        "{dirty} tab{} {} unsaved changes.",
                        if dirty == 1 { "" } else { "s" },
                        if dirty == 1 { "has" } else { "have" }
                    ),
                    "SAVE ALL",
                )
            }
        };

        let card = container(
            column![
                text(title).size(18).color(theme::TEXT),
                text(body).size(12).color(theme::MUTED),
                Space::new().height(Length::Fixed(18.0)),
                row![
                    Space::new().width(Length::Fill),
                    button(
                        text("CANCEL")
                            .size(10)
                            .font(Font::MONOSPACE)
                            .color(theme::MUTED),
                    )
                    .padding([8, 12])
                    .style(theme::subtle)
                    .on_press(Message::User(EditorMessage::ModalCancel)),
                    button(
                        text("DISCARD")
                            .size(10)
                            .font(Font::MONOSPACE)
                            .color(theme::RED),
                    )
                    .padding([8, 12])
                    .style(theme::subtle)
                    .on_press(Message::User(EditorMessage::ModalDiscard)),
                    button(
                        text(confirm)
                            .size(10)
                            .font(Font::MONOSPACE)
                            .color(theme::BG),
                    )
                    .padding([8, 12])
                    .style(theme::primary)
                    .on_press(Message::User(EditorMessage::ModalSave)),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            ]
            .spacing(4)
            .width(Length::Fixed(430.0)),
        )
        .style(theme::card)
        .padding([22, 22]);

        let layer = container(card)
            .width(Length::Fill)
            .height(Length::Fill)
            .center(Length::Fill)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(iced::Color::from_rgba(
                    0.0, 0.0, 0.0, 0.64,
                ))),
                ..Default::default()
            });
        Some(opaque(layer))
    }

    fn ime_cursor(&self, view: &EditorSnapshot) -> Option<Rectangle> {
        let row = view.view.cursor_row?;
        let row = view.view.rows.get(row)?;
        let column = row.cursor?;
        let scroll = if view.view.wrap {
            0
        } else {
            view.view.scroll_col
        };
        let x = TEXT_LEFT + column.saturating_sub(scroll) as f32 * CELL_WIDTH;
        let y = TEXT_TOP + view.view.cursor_row? as f32 * LINE_HEIGHT;
        Some(Rectangle::new(
            Point::new(x, y),
            Size::new(1.0, LINE_HEIGHT),
        ))
    }

    fn ime_active(&self, view: &EditorSnapshot) -> bool {
        !view.search.open && view.modal.is_none()
    }
}

impl Renderer<EditorSnapshot, EditorMessage, EditorLocal> for EditorRenderer {
    fn view<'a>(&'a self, view: &'a EditorSnapshot) -> Element<'a, Message> {
        let header = self.header(view);
        let tabs = self.tab_strip(view);
        let search = view.search.open.then(|| self.search_panel(view));
        let body = self.body(view);
        let status = self.status_bar(view);

        let mut content = column![header, tabs].width(Length::Fill);
        if let Some(search) = search {
            content = content.push(search);
        }
        content = content.push(body).push(status);

        let base: Element<'_, Message> = content.width(Length::Fill).height(Length::Fill).into();
        if let Some(modal) = self.modal_view(view) {
            stack(vec![base, modal])
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        } else {
            base
        }
    }

    fn subscription(&self, view: &EditorSnapshot) -> Subscription<Message> {
        let mut subscriptions = vec![
            keyboard::listen().filter_map(key_message),
            window::resize_events().map(|(_, size)| Message::Local(EditorLocal::Resize(size))),
            window::events().filter_map(|(id, event)| {
                matches!(event, window::Event::Rescaled(_))
                    .then_some(Message::Local(EditorLocal::Rescaled(id)))
            }),
            window::close_requests().map(|_| Message::User(EditorMessage::CloseRequested)),
            iced::event::listen_with(ime_message),
        ];
        if self.resize_deadline.is_some() {
            subscriptions.push(
                iced::time::every(Duration::from_millis(16))
                    .map(|_| Message::Local(EditorLocal::ResizeSettled)),
            );
        }
        if view.wrap_pending {
            subscriptions.push(
                iced::time::every(Duration::from_millis(16))
                    .map(|_| Message::User(EditorMessage::PollWrap)),
            );
        }
        Subscription::batch(subscriptions)
    }

    fn update(&mut self, local: EditorLocal, view: &EditorSnapshot) -> Task<Message> {
        match local {
            EditorLocal::Resize(size) => {
                self.window_size = size;
                if !view.defer_resize {
                    self.resize_deadline = None;
                    return self.viewport_task(view);
                }
                self.resize_deadline = Some(Instant::now() + Duration::from_millis(80));
                Task::none()
            }
            EditorLocal::Rescaled(id) => {
                window::size(id).map(|size| Message::Local(EditorLocal::Resize(size)))
            }
            EditorLocal::ResizeSettled => {
                if self
                    .resize_deadline
                    .is_some_and(|deadline| Instant::now() >= deadline)
                {
                    self.resize_deadline = None;
                    self.viewport_task(view)
                } else {
                    Task::none()
                }
            }
            EditorLocal::ImeOpened => {
                if self.ime_active(view) {
                    self.preedit = Some(String::new());
                }
                Task::none()
            }
            EditorLocal::ImePreedit(text) => {
                if self.ime_active(view) {
                    self.preedit = Some(text);
                }
                Task::none()
            }
            EditorLocal::ImeCommit(text) => {
                if self.ime_active(view) {
                    self.preedit = None;
                    Task::done(Message::User(EditorMessage::Insert(text)))
                } else {
                    Task::none()
                }
            }
            EditorLocal::ImeClosed => {
                self.preedit = None;
                Task::none()
            }
        }
    }

    fn on_snapshot_task(&mut self, view: &EditorSnapshot) -> Task<Message> {
        if !self.ime_active(view) {
            self.preedit = None;
        }
        let mut tasks = Vec::new();
        if self.last_viewport != Some(self.layout(view)) {
            tasks.push(self.viewport_task(view));
        }
        if let Some(focus) = view.focus
            && self.last_focus != Some(focus.id)
        {
            self.last_focus = Some(focus.id);
            let id = match focus.target {
                FocusTarget::Query => Self::query_id(),
                FocusTarget::Replacement => Self::replacement_id(),
            };
            tasks.push(operation::focus(id));
        }
        Task::batch(tasks)
    }

    fn title(&self, view: &EditorSnapshot) -> Option<String> {
        Some(view.title())
    }

    fn theme(&self, _view: &EditorSnapshot) -> Theme {
        Theme::Dark
    }
}

fn is_welcome(view: &EditorSnapshot) -> bool {
    view.view.path.is_none()
        && view.view.line_count == 1
        && view
            .view
            .rows
            .first()
            .is_some_and(|row| row.runs.is_empty())
}

fn truncate(value: &str, max: usize) -> String {
    let mut chars = value.chars();
    let mut output: String = chars.by_ref().take(max).collect();
    if chars.next().is_some() {
        output.push('…');
    }
    output
}

fn action_button(label: &str, message: EditorMessage, active: bool) -> Element<'_, Message> {
    button(text(label).size(10).font(Font::MONOSPACE).color(if active {
        theme::BG
    } else {
        theme::MUTED
    }))
    .padding([5, 8])
    .style(theme::active_toggle(active))
    .on_press(Message::User(message))
    .into()
}

fn key_message(event: keyboard::Event) -> Option<Message> {
    let keyboard::Event::KeyPressed { key, modifiers, .. } = event else {
        return None;
    };
    let shift = modifiers.shift();
    let command = modifiers.command();
    let user = Message::User;

    if command {
        if let Key::Character(character) = &key {
            match character.to_ascii_lowercase().as_str() {
                "n" => return Some(user(EditorMessage::New)),
                "o" => return Some(user(EditorMessage::Open)),
                "s" if shift => return Some(user(EditorMessage::SaveAs)),
                "s" => return Some(user(EditorMessage::Save)),
                "w" => return Some(user(EditorMessage::CloseActiveTab)),
                "f" => return Some(user(EditorMessage::Find)),
                "h" => return Some(user(EditorMessage::Replace)),
                "z" if shift => return Some(user(EditorMessage::Redo)),
                "z" => return Some(user(EditorMessage::Undo)),
                "y" => return Some(user(EditorMessage::Redo)),
                "c" => return Some(user(EditorMessage::Copy)),
                "x" => return Some(user(EditorMessage::Cut)),
                "v" => return Some(user(EditorMessage::Paste)),
                "a" => return Some(user(EditorMessage::SelectAll)),
                "q" => return Some(user(EditorMessage::Quit)),
                _ => {}
            }
        }
        return match key {
            Key::Named(Named::Home) => Some(user(EditorMessage::MoveDocumentStart(shift))),
            Key::Named(Named::End) => Some(user(EditorMessage::MoveDocumentEnd(shift))),
            Key::Named(Named::Tab) if shift => Some(user(EditorMessage::PreviousTab)),
            Key::Named(Named::Tab) => Some(user(EditorMessage::NextTab)),
            _ => None,
        };
    }

    if modifiers.alt()
        && let Key::Character(character) = &key
        && character.eq_ignore_ascii_case("z")
    {
        return Some(user(EditorMessage::ToggleWrap));
    }

    match key {
        Key::Named(Named::ArrowLeft) => Some(user(EditorMessage::MoveLeft(shift))),
        Key::Named(Named::ArrowRight) => Some(user(EditorMessage::MoveRight(shift))),
        Key::Named(Named::ArrowUp) => Some(user(EditorMessage::MoveUp(shift))),
        Key::Named(Named::ArrowDown) => Some(user(EditorMessage::MoveDown(shift))),
        Key::Named(Named::Home) => Some(user(EditorMessage::MoveHome(shift))),
        Key::Named(Named::End) => Some(user(EditorMessage::MoveEnd(shift))),
        Key::Named(Named::PageUp) => Some(user(EditorMessage::PageUp(shift))),
        Key::Named(Named::PageDown) => Some(user(EditorMessage::PageDown(shift))),
        Key::Named(Named::Backspace) => Some(user(EditorMessage::Backspace)),
        Key::Named(Named::Delete) => Some(user(EditorMessage::DeleteForward)),
        Key::Named(Named::Enter) => Some(user(EditorMessage::Enter)),
        Key::Named(Named::Tab) => Some(user(EditorMessage::Tab)),
        Key::Named(Named::F3) if shift => Some(user(EditorMessage::FindPrevious)),
        Key::Named(Named::F3) => Some(user(EditorMessage::FindNext)),
        Key::Named(Named::F4) => Some(user(EditorMessage::FocusReplacement)),
        Key::Character(c) if modifiers.control() && c.as_str().eq_ignore_ascii_case("r") => {
            Some(user(EditorMessage::FocusReplacement))
        }
        Key::Named(Named::Escape) => Some(user(EditorMessage::Escape)),
        Key::Character(character)
            if !modifiers.alt() && !modifiers.control() && !modifiers.logo() =>
        {
            Some(user(EditorMessage::Insert(character.to_string())))
        }
        _ => None,
    }
}

fn ime_message(
    event: iced::event::Event,
    _status: iced::event::Status,
    _window: window::Id,
) -> Option<Message> {
    match event {
        iced::event::Event::InputMethod(input) => match input {
            iced::advanced::input_method::Event::Opened => {
                Some(Message::Local(EditorLocal::ImeOpened))
            }
            iced::advanced::input_method::Event::Preedit(text, _selection) => {
                Some(Message::Local(EditorLocal::ImePreedit(text)))
            }
            iced::advanced::input_method::Event::Commit(text) => {
                Some(Message::Local(EditorLocal::ImeCommit(text)))
            }
            iced::advanced::input_method::Event::Closed => {
                Some(Message::Local(EditorLocal::ImeClosed))
            }
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_map_keeps_semantic_messages_out_of_toolkit_types() {
        let event = keyboard::Event::KeyPressed {
            key: Key::Character("f".into()),
            modified_key: Key::Character("f".into()),
            physical_key: keyboard::key::Physical::Unidentified(
                keyboard::key::NativeCode::Unidentified,
            ),
            location: keyboard::Location::Standard,
            modifiers: keyboard::Modifiers::COMMAND,
            text: None,
            repeat: false,
        };
        assert!(matches!(
            key_message(event),
            Some(Message::User(EditorMessage::Find))
        ));
    }
}
