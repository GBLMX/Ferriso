//! Ratatui frontend for the actor-owned editor.
//!
//! This adapter owns terminal-local state only: the text being edited in the
//! find bar, the last terminal size, and the area used for cursor placement.
//! Tabs, search matches, and save state come from [`EditorSnapshot`].

use ferriso_editor::{TokenKind, VisualLine};
use ferriso_tui::{
    Outcome, Renderer,
    crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    ratatui::{
        Frame,
        layout::{Alignment, Constraint, Layout, Rect},
        style::{Color, Modifier, Style},
        text::{Line, Span},
        widgets::{Clear, Paragraph},
    },
};

use crate::model::{EditorMessage, EditorSnapshot, FocusTarget, Modal};

const GUTTER: usize = 6;

pub struct EditorTui {
    area: Rect,
    query: String,
    replacement: String,
    search_focus: FocusTarget,
    search_open: bool,
    last_viewport: Option<(usize, usize)>,
    pending_viewport: Option<(usize, usize)>,
}

impl Default for EditorTui {
    fn default() -> Self {
        Self {
            area: Rect::default(),
            query: String::new(),
            replacement: String::new(),
            search_focus: FocusTarget::Query,
            search_open: false,
            last_viewport: None,
            pending_viewport: None,
        }
    }
}

impl EditorTui {
    pub fn new() -> Self {
        Self::default()
    }

    fn search_height(&self, view: &EditorSnapshot) -> u16 {
        if !view.search.open {
            0
        } else if view.search.replace {
            2
        } else {
            1
        }
    }

    fn viewport(&self, view: &EditorSnapshot) -> (usize, usize) {
        let search = self.search_height(view);
        (
            self.area.height.saturating_sub(3 + search).max(1) as usize,
            self.area.width.saturating_sub(GUTTER as u16 + 1).max(10) as usize,
        )
    }

    fn body_area(&self, view: &EditorSnapshot) -> Rect {
        let search = self.search_height(view);
        let chunks = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(search),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(self.area);
        chunks[3]
    }

    fn draw_header(&self, view: &EditorSnapshot, frame: &mut Frame, area: Rect) {
        let name = view.active_name();
        let dirty = if view.view.dirty { " ●" } else { "" };
        let line = Line::from(vec![
            Span::styled(
                " FERRISO ",
                Style::default()
                    .fg(Color::Rgb(18, 20, 24))
                    .bg(Color::Rgb(110, 190, 255))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" EDITOR ", Style::default().fg(Color::Rgb(168, 178, 194))),
            Span::styled(
                format!("{name}{dirty}"),
                Style::default().fg(Color::Rgb(232, 236, 242)),
            ),
            Span::styled(
                if view.view.wrap { "  wrap" } else { "  nowrap" },
                Style::default().fg(Color::Rgb(110, 190, 255)),
            ),
            Span::styled(
                "   Ctrl+Q quit",
                Style::default().fg(Color::Rgb(88, 96, 110)),
            ),
        ]);
        frame.render_widget(Paragraph::new(line), area);
    }

    fn draw_tabs(&self, view: &EditorSnapshot, frame: &mut Frame, area: Rect) {
        let mut spans = Vec::new();
        for (index, tab) in view.tabs.iter().enumerate() {
            let active = index == view.active;
            let name = format!(" {}{} ", tab.name, if tab.dirty { " ●" } else { "" });
            spans.push(Span::styled(
                name,
                if active {
                    Style::default()
                        .fg(Color::Rgb(232, 236, 242))
                        .bg(Color::Rgb(38, 44, 56))
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Rgb(130, 139, 153))
                },
            ));
        }
        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    }

    fn draw_search(&self, view: &EditorSnapshot, frame: &mut Frame, area: Rect) {
        let count = if view.search.query.is_empty() {
            String::new()
        } else if view.search.matches.is_empty() {
            "   no matches".to_string()
        } else {
            match view.search.current {
                Some(index) => format!("   {}/{}", index + 1, view.search.matches.len()),
                None => format!("   {} matches", view.search.matches.len()),
            }
        };
        let active_style = Style::default()
            .fg(Color::Rgb(232, 236, 242))
            .bg(Color::Rgb(30, 35, 44));
        let query_style = if self.search_focus == FocusTarget::Query {
            active_style
        } else {
            Style::default().fg(Color::Rgb(168, 178, 194))
        };
        let line = Line::from(vec![
            Span::styled(" FIND ", Style::default().fg(Color::Rgb(110, 190, 255))),
            Span::styled(format!(" {} ", self.query), query_style),
            Span::styled(count, Style::default().fg(Color::Rgb(130, 139, 153))),
            Span::styled(
                "  Tab field · Enter next · Esc close",
                Style::default().fg(Color::Rgb(88, 96, 110)),
            ),
        ]);
        frame.render_widget(Paragraph::new(line), Rect { height: 1, ..area });

        if view.search.replace {
            let replacement_style = if self.search_focus == FocusTarget::Replacement {
                active_style
            } else {
                Style::default().fg(Color::Rgb(168, 178, 194))
            };
            let line = Line::from(vec![
                Span::styled(" WITH ", Style::default().fg(Color::Rgb(110, 190, 255))),
                Span::styled(format!(" {} ", self.replacement), replacement_style),
                Span::styled(
                    "  Enter replace · Ctrl+Alt+Enter all",
                    Style::default().fg(Color::Rgb(88, 96, 110)),
                ),
            ]);
            frame.render_widget(
                Paragraph::new(line),
                Rect {
                    y: area.y + 1,
                    height: 1,
                    ..area
                },
            );
        }
    }

    fn draw_body(&self, view: &EditorSnapshot, frame: &mut Frame, area: Rect) {
        let scroll_col = if view.view.wrap {
            0
        } else {
            view.view.scroll_col
        };
        let mut lines = Vec::with_capacity(view.view.rows.len());
        for row in &view.view.rows {
            lines.push(self.line_for(row, area.width as usize, scroll_col));
        }
        frame.render_widget(Paragraph::new(lines), area);
    }

    fn line_for(&self, row: &VisualLine, width: usize, scroll_col: usize) -> Line<'static> {
        let number = if row.continuation {
            "     ".to_string()
        } else {
            format!("{:>4} ", row.number)
        };
        let mut spans = vec![Span::styled(
            number,
            Style::default().fg(Color::Rgb(88, 96, 110)),
        )];
        let mut used = GUTTER;
        for run in &row.runs {
            if used >= width {
                break;
            }
            let skip = scroll_col.saturating_sub(run.column);
            let text: String = run.text.chars().skip(skip).collect();
            if text.is_empty() {
                continue;
            }
            let room = width.saturating_sub(used);
            let text: String = text.chars().take(room).collect();
            used += text.chars().count();
            let selected = row.selection.is_some_and(|(start, end)| {
                let run_end = run.column + run.width;
                start < run_end && end > run.column
            });
            let mut style = Style::default().fg(token_color(run.kind));
            if selected {
                style = style.bg(Color::Rgb(48, 66, 94));
            }
            spans.push(Span::styled(text, style));
        }
        if row.selection.is_some() && used < width {
            spans.push(Span::styled(
                " ",
                Style::default().bg(Color::Rgb(48, 66, 94)),
            ));
        }
        Line::from(spans)
    }

    fn draw_status(&self, view: &EditorSnapshot, frame: &mut Frame, area: Rect) {
        let view = &view.view;
        let ending = match view.line_ending {
            ferriso_editor::LineEnding::Lf => "LF",
            ferriso_editor::LineEnding::CrLf => "CRLF",
        };
        let status = if view.status.is_empty() {
            format!(
                " Ln {}, Col {}  ·  {} lines  ·  {}  ·  UTF-8  ·  {} ",
                view.cursor_line,
                view.cursor_column,
                view.line_count,
                view.language.unwrap_or("plain"),
                ending
            )
        } else {
            format!(" {} ", view.status)
        };
        let state = if view.dirty { " UNSAVED " } else { " SAVED " };
        let line = Line::from(vec![
            Span::styled(
                status,
                Style::default().fg(if view.status.is_empty() {
                    Color::Rgb(130, 139, 153)
                } else {
                    Color::Rgb(255, 190, 90)
                }),
            ),
            Span::styled(
                state,
                Style::default().fg(if view.dirty {
                    Color::Rgb(255, 190, 90)
                } else {
                    Color::Rgb(120, 220, 160)
                }),
            ),
        ]);
        frame.render_widget(Paragraph::new(line), area);
    }

    fn draw_modal(&self, view: &EditorSnapshot, frame: &mut Frame) {
        let Some(modal) = view.modal else {
            return;
        };
        let (title, body) = match modal {
            Modal::CloseTab(index) => {
                let name = view
                    .tabs
                    .get(index)
                    .map_or("this file", |tab| tab.name.as_str());
                (
                    format!("Close {name}?"),
                    "Save changes before closing?  [S] save  [D] discard  [Esc] cancel",
                )
            }
            Modal::Quit => (
                "Quit Ferriso?".to_string(),
                "There are unsaved tabs.  [S] save all  [D] discard  [Esc] cancel",
            ),
        };
        let area = centered(frame.area(), 64, 6);
        frame.render_widget(Clear, area);
        let text = vec![
            Line::from(Span::styled(
                title,
                Style::default()
                    .fg(Color::Rgb(232, 236, 242))
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                body,
                Style::default().fg(Color::Rgb(168, 178, 194)),
            )),
        ];
        let card = Paragraph::new(text).alignment(Alignment::Center).block(
            ferriso_tui::ratatui::widgets::Block::bordered()
                .border_style(Style::default().fg(Color::Rgb(110, 190, 255)))
                .style(Style::default().bg(Color::Rgb(18, 20, 24))),
        );
        frame.render_widget(card, area);
    }

    fn place_cursor(&self, view: &EditorSnapshot, frame: &mut Frame) {
        if view.search.open {
            let area = self.area;
            let search_y = area.y + 2;
            let (prefix, text) = if self.search_focus == FocusTarget::Query {
                (7, &self.query)
            } else {
                (7, &self.replacement)
            };
            let y = if self.search_focus == FocusTarget::Replacement {
                search_y + 1
            } else {
                search_y
            };
            let x = area.x + prefix + text.chars().count() as u16;
            frame.set_cursor_position((x.min(area.right().saturating_sub(1)), y));
            return;
        }
        if view.modal.is_some() {
            return;
        }
        let Some(row_index) = view.view.cursor_row else {
            return;
        };
        let Some(row) = view.view.rows.get(row_index) else {
            return;
        };
        let Some(column) = row.cursor else {
            return;
        };
        let scroll = if view.view.wrap {
            0
        } else {
            view.view.scroll_col
        };
        let body = self.body_area(view);
        let x = body.x + GUTTER as u16 + column.saturating_sub(scroll) as u16;
        let y = body.y + row_index as u16;
        if x < body.right() && y < body.bottom() {
            frame.set_cursor_position((x, y));
        }
    }

    fn edit_search(&mut self, key: KeyEvent) -> Option<EditorMessage> {
        let KeyEvent {
            code, modifiers, ..
        } = key;
        let buffer = if self.search_focus == FocusTarget::Replacement {
            &mut self.replacement
        } else {
            &mut self.query
        };
        match code {
            KeyCode::Backspace => {
                buffer.pop();
            }
            KeyCode::Delete => {}
            KeyCode::Char(character)
                if !modifiers.intersects(
                    KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER,
                ) =>
            {
                buffer.push(character)
            }
            _ => return None,
        }
        Some(if self.search_focus == FocusTarget::Replacement {
            EditorMessage::Replacement(buffer.clone())
        } else {
            EditorMessage::SearchQuery(buffer.clone())
        })
    }
}

impl Renderer<EditorSnapshot, EditorMessage> for EditorTui {
    fn draw(&mut self, view: &EditorSnapshot, frame: &mut Frame) {
        self.area = frame.area();
        if self.last_viewport.is_none() {
            self.last_viewport = Some(self.viewport(view));
        }
        let search = self.search_height(view);
        let chunks = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(search),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(self.area);

        self.draw_header(view, frame, chunks[0]);
        self.draw_tabs(view, frame, chunks[1]);
        if search > 0 {
            self.draw_search(view, frame, chunks[2]);
        }
        self.draw_body(view, frame, chunks[3]);
        self.draw_status(view, frame, chunks[4]);
        self.draw_modal(view, frame);
        self.place_cursor(view, frame);
    }

    fn handle(&mut self, view: &EditorSnapshot, event: Event) -> Outcome<EditorMessage> {
        if let Event::Resize(columns, rows) = event {
            self.area = Rect::new(0, 0, columns, rows);
            let (rows, columns) = self.viewport(view);
            self.last_viewport = Some((rows, columns));
            self.pending_viewport = None;
            return Outcome::Message(EditorMessage::Viewport { rows, columns });
        }
        let Event::Key(mut key) = event else {
            return Outcome::Ignored;
        };
        if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            return Outcome::Ignored;
        }

        if view.modal.is_some() {
            return match key.code {
                KeyCode::Char('s') | KeyCode::Enter => Outcome::Message(EditorMessage::ModalSave),
                KeyCode::Char('d') => Outcome::Message(EditorMessage::ModalDiscard),
                KeyCode::Esc => Outcome::Message(EditorMessage::ModalCancel),
                _ => Outcome::Ignored,
            };
        }

        if view.search.open
            && let Some(message) = self.search_key(view, key)
        {
            return Outcome::Message(message);
        }

        // Crossterm reports `Ctrl+Shift+S` as `Char('S')` with SHIFT set.
        // Normalize only control chords; plain shifted text must stay intact.
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && let KeyCode::Char(character) = key.code
            && character.is_ascii_uppercase()
        {
            key.code = KeyCode::Char(character.to_ascii_lowercase());
        }

        match key {
            KeyEvent {
                code: KeyCode::Char('q'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => Outcome::Message(EditorMessage::Quit),
            KeyEvent {
                code: KeyCode::Char('s'),
                modifiers,
                ..
            } if modifiers.contains(KeyModifiers::CONTROL | KeyModifiers::SHIFT) => {
                Outcome::Message(EditorMessage::SaveAs)
            }
            KeyEvent {
                code: KeyCode::Char('s'),
                modifiers,
                ..
            } if modifiers.contains(KeyModifiers::CONTROL) => Outcome::Message(EditorMessage::Save),
            KeyEvent {
                code: KeyCode::Char('o'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => Outcome::Message(EditorMessage::Open),
            KeyEvent {
                code: KeyCode::Char('n'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => Outcome::Message(EditorMessage::New),
            KeyEvent {
                code: KeyCode::Char('w'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => Outcome::Message(EditorMessage::CloseActiveTab),
            KeyEvent {
                code: KeyCode::Char('f'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => Outcome::Message(EditorMessage::Find),
            KeyEvent {
                code: KeyCode::Char('h'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => Outcome::Message(EditorMessage::Replace),
            KeyEvent {
                code: KeyCode::Char('z'),
                modifiers,
                ..
            } if modifiers.contains(KeyModifiers::CONTROL | KeyModifiers::SHIFT) => {
                Outcome::Message(EditorMessage::Redo)
            }
            KeyEvent {
                code: KeyCode::Char('y'),
                modifiers,
                ..
            } if modifiers.contains(KeyModifiers::CONTROL) => Outcome::Message(EditorMessage::Redo),
            KeyEvent {
                code: KeyCode::Char('z'),
                modifiers,
                ..
            } if modifiers.contains(KeyModifiers::CONTROL) => Outcome::Message(EditorMessage::Undo),
            KeyEvent {
                code: KeyCode::Char('a'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => Outcome::Message(EditorMessage::SelectAll),
            KeyEvent {
                code: KeyCode::Char('c'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => Outcome::Message(EditorMessage::Copy),
            KeyEvent {
                code: KeyCode::Char('x'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => Outcome::Message(EditorMessage::Cut),
            KeyEvent {
                code: KeyCode::Char('v'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => Outcome::Message(EditorMessage::Paste),
            KeyEvent {
                code: KeyCode::Char('z'),
                modifiers: KeyModifiers::ALT,
                ..
            } => Outcome::Message(EditorMessage::ToggleWrap),
            KeyEvent {
                code: KeyCode::Up,
                modifiers,
                ..
            } => Outcome::Message(EditorMessage::MoveUp(
                modifiers.contains(KeyModifiers::SHIFT),
            )),
            KeyEvent {
                code: KeyCode::Down,
                modifiers,
                ..
            } => Outcome::Message(EditorMessage::MoveDown(
                modifiers.contains(KeyModifiers::SHIFT),
            )),
            KeyEvent {
                code: KeyCode::Left,
                modifiers,
                ..
            } => Outcome::Message(EditorMessage::MoveLeft(
                modifiers.contains(KeyModifiers::SHIFT),
            )),
            KeyEvent {
                code: KeyCode::Right,
                modifiers,
                ..
            } => Outcome::Message(EditorMessage::MoveRight(
                modifiers.contains(KeyModifiers::SHIFT),
            )),
            KeyEvent {
                code: KeyCode::Home,
                modifiers,
                ..
            } => Outcome::Message(EditorMessage::MoveHome(
                modifiers.contains(KeyModifiers::SHIFT),
            )),
            KeyEvent {
                code: KeyCode::End,
                modifiers,
                ..
            } => Outcome::Message(EditorMessage::MoveEnd(
                modifiers.contains(KeyModifiers::SHIFT),
            )),
            KeyEvent {
                code: KeyCode::PageUp,
                modifiers,
                ..
            } => Outcome::Message(EditorMessage::PageUp(
                modifiers.contains(KeyModifiers::SHIFT),
            )),
            KeyEvent {
                code: KeyCode::PageDown,
                modifiers,
                ..
            } => Outcome::Message(EditorMessage::PageDown(
                modifiers.contains(KeyModifiers::SHIFT),
            )),
            KeyEvent {
                code: KeyCode::Backspace,
                ..
            } => Outcome::Message(EditorMessage::Backspace),
            KeyEvent {
                code: KeyCode::Delete,
                ..
            } => Outcome::Message(EditorMessage::DeleteForward),
            KeyEvent {
                code: KeyCode::Enter,
                ..
            } => Outcome::Message(EditorMessage::Enter),
            KeyEvent {
                code: KeyCode::BackTab,
                ..
            } => Outcome::Message(EditorMessage::PreviousTab),
            KeyEvent {
                code: KeyCode::Tab,
                modifiers,
                ..
            } if modifiers.contains(KeyModifiers::CONTROL | KeyModifiers::SHIFT) => {
                Outcome::Message(EditorMessage::PreviousTab)
            }
            KeyEvent {
                code: KeyCode::Tab,
                modifiers,
                ..
            } if modifiers.contains(KeyModifiers::CONTROL) => {
                Outcome::Message(EditorMessage::NextTab)
            }
            KeyEvent {
                code: KeyCode::Tab,
                modifiers,
                ..
            } if modifiers.contains(KeyModifiers::SHIFT) => {
                Outcome::Message(EditorMessage::PreviousTab)
            }
            KeyEvent {
                code: KeyCode::Tab, ..
            } => Outcome::Message(EditorMessage::Tab),
            KeyEvent {
                code: KeyCode::F(3),
                modifiers,
                ..
            } => Outcome::Message(if modifiers.contains(KeyModifiers::SHIFT) {
                EditorMessage::FindPrevious
            } else {
                EditorMessage::FindNext
            }),
            KeyEvent {
                code: KeyCode::Esc, ..
            } => Outcome::Message(EditorMessage::Escape),
            KeyEvent {
                code: KeyCode::Char(character),
                modifiers,
                ..
            } if !modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER) =>
            {
                Outcome::Message(EditorMessage::Insert(character.to_string()))
            }
            _ => Outcome::Ignored,
        }
    }

    fn on_snapshot(&mut self, view: &EditorSnapshot) {
        if view.search.open && !self.search_open {
            self.query = view.search.query.clone();
            self.replacement = view.search.replacement.clone();
            self.search_focus = if view.search.replace && !view.search.query.is_empty() {
                FocusTarget::Replacement
            } else {
                FocusTarget::Query
            };
        }
        self.search_open = view.search.open;
        if !view.search.open {
            self.query.clear();
            self.replacement.clear();
        }
        if self.area.width > 0 {
            let viewport = self.viewport(view);
            if self.last_viewport != Some(viewport) {
                self.last_viewport = Some(viewport);
                self.pending_viewport = Some(viewport);
            }
        }
    }

    fn on_snapshot_message(&mut self, _view: &EditorSnapshot) -> Option<EditorMessage> {
        self.pending_viewport
            .take()
            .map(|(rows, columns)| EditorMessage::Viewport { rows, columns })
    }
}

impl EditorTui {
    fn search_key(&mut self, view: &EditorSnapshot, key: KeyEvent) -> Option<EditorMessage> {
        match key.code {
            KeyCode::Esc => Some(EditorMessage::CloseSearch),
            KeyCode::Tab if view.search.replace => {
                self.search_focus = match self.search_focus {
                    FocusTarget::Query => FocusTarget::Replacement,
                    FocusTarget::Replacement => FocusTarget::Query,
                };
                Some(EditorMessage::Tab)
            }
            KeyCode::Up => Some(EditorMessage::FindPrevious),
            KeyCode::Down => Some(EditorMessage::FindNext),
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::ALT) => {
                Some(EditorMessage::ReplaceAll)
            }
            KeyCode::Enter if self.search_focus == FocusTarget::Replacement => {
                Some(EditorMessage::ReplaceCurrent)
            }
            KeyCode::Enter => Some(EditorMessage::FindNext),
            _ => self.edit_search(key),
        }
    }
}

fn token_color(kind: TokenKind) -> Color {
    match kind {
        TokenKind::Plain => Color::Rgb(220, 226, 235),
        TokenKind::Comment => Color::Rgb(104, 118, 136),
        TokenKind::String => Color::Rgb(190, 214, 140),
        TokenKind::Number => Color::Rgb(230, 180, 120),
        TokenKind::Keyword => Color::Rgb(190, 150, 240),
        TokenKind::Function => Color::Rgb(120, 190, 250),
        TokenKind::Type => Color::Rgb(120, 210, 200),
        TokenKind::Variable => Color::Rgb(225, 225, 170),
        TokenKind::Constant => Color::Rgb(240, 160, 140),
        TokenKind::Markup => Color::Rgb(200, 210, 220),
    }
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use ferriso_core::App;
    use ferriso_tui::ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::model::EditorModel;

    #[test]
    fn tui_draws_the_actor_snapshot_without_panicking() {
        let model = EditorModel::new(None);
        let view = model.view();
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("terminal");
        let mut tui = EditorTui::new();
        terminal
            .draw(|frame| tui.draw(&view, frame))
            .expect("draw editor");
        let buffer = terminal.backend().buffer();
        let text: String = buffer.content().iter().map(|cell| cell.symbol()).collect();
        assert!(text.contains("FERRISO"));
        assert!(text.contains("Untitled") || text.contains("untitled"));
    }

    #[test]
    fn ctrl_q_is_semantic_so_unsaved_work_can_be_confirmed() {
        let model = EditorModel::new(None);
        let view = model.view();
        let mut tui = EditorTui::new();
        let outcome = tui.handle(
            &view,
            Event::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL)),
        );
        assert!(matches!(outcome, Outcome::Message(EditorMessage::Quit)));
    }

    #[test]
    fn opening_search_publishes_a_shorter_viewport_once() {
        let mut model = EditorModel::new(None);
        let view = model.view();
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("terminal");
        let mut tui = EditorTui::new();
        terminal
            .draw(|frame| tui.draw(&view, frame))
            .expect("draw editor");

        model.update(EditorMessage::Find);
        let view = model.view();
        tui.on_snapshot(&view);
        assert!(matches!(
            tui.on_snapshot_message(&view),
            Some(EditorMessage::Viewport {
                rows: 26,
                columns: 93
            })
        ));
        assert!(tui.on_snapshot_message(&view).is_none());
    }

    #[test]
    fn resize_uses_the_new_terminal_size() {
        let model = EditorModel::new(None);
        let view = model.view();
        let mut tui = EditorTui::new();
        tui.area = Rect::new(0, 0, 100, 30);
        let outcome = tui.handle(&view, Event::Resize(80, 24));
        assert!(matches!(
            outcome,
            Outcome::Message(EditorMessage::Viewport {
                rows: 21,
                columns: 73
            })
        ));
    }

    #[test]
    fn shifted_control_chords_keep_their_shortcut() {
        let model = EditorModel::new(None);
        let view = model.view();
        let mut tui = EditorTui::new();
        let outcome = tui.handle(
            &view,
            Event::Key(KeyEvent::new(
                KeyCode::Char('S'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT,
            )),
        );
        assert!(matches!(outcome, Outcome::Message(EditorMessage::SaveAs)));
    }

    #[test]
    fn search_does_not_swallow_control_shortcuts_as_text() {
        let mut model = EditorModel::new(None);
        model.update(EditorMessage::Find);
        let view = model.view();
        let mut tui = EditorTui::new();
        tui.on_snapshot(&view);
        let outcome = tui.handle(
            &view,
            Event::Key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL)),
        );
        assert!(matches!(outcome, Outcome::Message(EditorMessage::Save)));
        assert!(tui.query.is_empty());
    }
    #[test]
    fn empty_buffer_places_the_terminal_cursor() {
        let model = EditorModel::new(None);
        let view = model.view();
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("terminal");
        let mut tui = EditorTui::new();
        terminal
            .draw(|frame| tui.draw(&view, frame))
            .expect("draw editor");

        let position = terminal.get_cursor_position().expect("cursor position");
        assert!(position.x >= 6, "cursor x was {}", position.x);
        assert!(position.y >= 2, "cursor y was {}", position.y);
    }
}
