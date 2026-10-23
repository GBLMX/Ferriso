//! Terminal frontend for the yazi-desktop actor.
//!
//! The renderer owns only terminal-local state: the filter draft and the last
//! drawn area. Directories, selection, previews, and history come from
//! [`ViewModel`].

use ferriso_tui::{
    Outcome, Renderer,
    crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    ratatui::{
        Frame,
        layout::{Alignment, Constraint, Layout, Rect},
        style::{Color, Modifier, Style},
        text::{Line, Span},
        widgets::{Paragraph, Wrap},
    },
};

use crate::app::{Message, ViewModel};

const SIDEBAR_WIDTH: u16 = 22;

#[derive(Default)]
pub struct YaziTui {
    area: Rect,
    filter: Option<String>,
}

impl YaziTui {
    pub fn new() -> Self {
        Self::default()
    }

    fn draw_header(&self, view: &ViewModel, frame: &mut Frame, area: Rect) {
        let loading = if view.loading { "  loading…" } else { "" };
        let line = Line::from(vec![
            Span::styled(
                " YAZI ",
                Style::default()
                    .fg(Color::Rgb(18, 20, 24))
                    .bg(Color::Rgb(110, 190, 255))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" {}{}", view.cwd, loading),
                Style::default().fg(Color::Rgb(220, 226, 235)),
            ),
        ]);
        frame.render_widget(Paragraph::new(line), area);
    }

    fn draw_places(&self, view: &ViewModel, frame: &mut Frame, area: Rect) {
        let lines = view
            .places
            .iter()
            .map(|place| {
                let style = if place.active {
                    Style::default()
                        .fg(Color::Rgb(110, 190, 255))
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Rgb(130, 139, 153))
                };
                Line::from(Span::styled(format!(" {} ", place.label), style))
            })
            .collect::<Vec<_>>();
        frame.render_widget(Paragraph::new(lines), area);
    }

    fn draw_list(&self, view: &ViewModel, frame: &mut Frame, area: Rect) {
        let height = area.height as usize;
        if height == 0 {
            return;
        }
        let offset = view.selected.map_or(0, |selected| {
            selected
                .saturating_sub(height.saturating_sub(1))
                .min(view.rows.len().saturating_sub(height))
        });
        let lines = view
            .rows
            .iter()
            .skip(offset)
            .take(height)
            .map(|row| {
                let marker = if row.selected { "▌" } else { " " };
                let mut style = Style::default().fg(kind_color(row.kind));
                if row.selected {
                    style = style
                        .bg(Color::Rgb(38, 48, 64))
                        .add_modifier(Modifier::BOLD);
                }
                let detail = truncate(
                    &row.detail,
                    area.width
                        .saturating_sub(row.name.chars().count() as u16 + 5)
                        as usize,
                );
                Line::from(vec![
                    Span::styled(marker, Style::default().fg(Color::Rgb(110, 190, 255))),
                    Span::styled(
                        format!("{:<width$} ", truncate(&row.name, 34), width = 34),
                        style,
                    ),
                    Span::styled(
                        detail,
                        Style::default()
                            .fg(Color::Rgb(105, 114, 128))
                            .bg(if row.selected {
                                Color::Rgb(38, 48, 64)
                            } else {
                                Color::Reset
                            }),
                    ),
                ])
            })
            .collect::<Vec<_>>();
        frame.render_widget(Paragraph::new(lines), area);
    }

    fn draw_preview(&self, view: &ViewModel, frame: &mut Frame, area: Rect) {
        let preview = &view.preview;
        let mut lines = vec![
            Line::from(Span::styled(
                truncate(&preview.title, area.width as usize),
                Style::default()
                    .fg(Color::Rgb(232, 236, 242))
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                truncate(&preview.subtitle, area.width as usize),
                Style::default().fg(Color::Rgb(130, 139, 153)),
            )),
        ];
        for (key, value) in &preview.meta {
            lines.push(Line::from(Span::styled(
                truncate(&format!("{key}: {value}"), area.width as usize),
                Style::default().fg(Color::Rgb(110, 190, 255)),
            )));
        }
        if preview.image.is_some() {
            lines.push(Line::from(Span::styled(
                "[image preview unavailable in the terminal]",
                Style::default().fg(Color::Rgb(105, 114, 128)),
            )));
        } else if !preview.body.is_empty() {
            lines.push(Line::from(""));
            for line in preview.body.lines() {
                lines.push(Line::from(line.to_string()));
            }
        }
        let paragraph = Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .alignment(Alignment::Left);
        frame.render_widget(paragraph, area);
    }

    fn draw_footer(&self, view: &ViewModel, frame: &mut Frame, area: Rect) {
        let filter_height = u16::from(self.filter.is_some());
        let chunks = Layout::vertical([Constraint::Length(filter_height), Constraint::Length(1)])
            .split(area);
        if let Some(filter) = &self.filter {
            let line = Line::from(vec![
                Span::styled(" / ", Style::default().fg(Color::Rgb(110, 190, 255))),
                Span::styled(
                    filter.clone(),
                    Style::default().fg(Color::Rgb(232, 236, 242)),
                ),
            ]);
            frame.render_widget(Paragraph::new(line), chunks[0]);
            frame.set_cursor_position((
                chunks[0].x + 3 + filter.chars().count() as u16,
                chunks[0].y,
            ));
        }
        let style = if view.error {
            Style::default().fg(Color::Rgb(240, 120, 120))
        } else {
            Style::default().fg(Color::Rgb(130, 139, 153))
        };
        let hints = " ↑↓ move  Enter open  h back  g home  / filter  . hidden  r reload  q quit ";
        let line = Line::from(vec![
            Span::styled(format!(" {} ", view.status), style),
            Span::styled(hints, Style::default().fg(Color::Rgb(88, 96, 110))),
        ]);
        frame.render_widget(Paragraph::new(line), chunks[chunks.len() - 1]);
    }

    fn start_filter(&mut self, view: &ViewModel) {
        self.filter = Some(view.filter.clone());
    }

    fn edit_filter(&mut self, code: KeyCode) -> Option<Message> {
        match code {
            KeyCode::Enter => {
                self.filter = None;
                None
            }
            KeyCode::Esc => {
                self.filter = None;
                Some(Message::FilterChanged(String::new()))
            }
            KeyCode::Backspace => {
                let filter = self.filter.as_mut()?;
                filter.pop();
                Some(Message::FilterChanged(filter.clone()))
            }
            KeyCode::Char(character) => {
                let filter = self.filter.as_mut()?;
                filter.push(character);
                Some(Message::FilterChanged(filter.clone()))
            }
            _ => None,
        }
    }

    fn filter_key(&mut self, key: KeyEvent) -> Option<Message> {
        let code = key.code;
        if !key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
        {
            self.edit_filter(code)
        } else {
            None
        }
    }

    fn handle_filter_paste(&mut self, text: &str) -> Option<Message> {
        let filter = self.filter.as_mut()?;
        filter.push_str(text);
        Some(Message::FilterChanged(filter.clone()))
    }
}

impl Renderer<ViewModel, Message> for YaziTui {
    fn draw(&mut self, view: &ViewModel, frame: &mut Frame) {
        self.area = frame.area();
        let [header, body, footer] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(if self.filter.is_some() { 2 } else { 1 }),
        ])
        .areas(self.area);

        self.draw_header(view, frame, header);
        if self.area.width >= 90 {
            let [sidebar, list, preview] = Layout::horizontal([
                Constraint::Length(SIDEBAR_WIDTH),
                Constraint::Percentage(58),
                Constraint::Percentage(42),
            ])
            .areas(body);
            self.draw_places(view, frame, sidebar);
            self.draw_list(view, frame, list);
            self.draw_preview(view, frame, preview);
        } else {
            let [sidebar, list] =
                Layout::horizontal([Constraint::Length(SIDEBAR_WIDTH), Constraint::Min(20)])
                    .areas(body);
            self.draw_places(view, frame, sidebar);
            self.draw_list(view, frame, list);
        }
        self.draw_footer(view, frame, footer);
    }

    fn handle(&mut self, view: &ViewModel, event: Event) -> Outcome<Message> {
        let Event::Key(key) = event else {
            return match event {
                Event::Paste(text) if self.filter.is_some() => self
                    .handle_filter_paste(&text)
                    .map_or(Outcome::Ignored, Outcome::Message),
                _ => Outcome::Ignored,
            };
        };
        if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            return Outcome::Ignored;
        }
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
        {
            return if key.modifiers.contains(KeyModifiers::CONTROL)
                && key.code == KeyCode::Char('q')
            {
                Outcome::Quit
            } else {
                Outcome::Ignored
            };
        }
        if self.filter.is_some() {
            match key.code {
                KeyCode::Esc => {
                    self.filter = None;
                    return Outcome::Message(Message::FilterChanged(String::new()));
                }
                KeyCode::Enter => {
                    self.filter = None;
                    return Outcome::Ignored;
                }
                _ => {
                    if let Some(message) = self.filter_key(key) {
                        return Outcome::Message(message);
                    }
                }
            }
        }

        let message = match key.code {
            KeyCode::Char('q') => return Outcome::Quit,
            KeyCode::Up | KeyCode::Char('k') => Message::MoveSelection(-1),
            KeyCode::Down | KeyCode::Char('j') => Message::MoveSelection(1),
            KeyCode::PageUp => Message::MoveSelection(-10),
            KeyCode::PageDown => Message::MoveSelection(10),
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => Message::Activate,
            KeyCode::Left | KeyCode::Char('h') => Message::Up,
            KeyCode::Backspace => Message::Back,
            KeyCode::Char('g') => Message::Home,
            KeyCode::Char('r') => Message::Refresh,
            KeyCode::Char('.') => Message::ToggleHidden,
            KeyCode::Char('/') => {
                self.start_filter(view);
                return Outcome::Ignored;
            }
            KeyCode::Esc if !view.filter.is_empty() => Message::FilterChanged(String::new()),
            KeyCode::Esc => return Outcome::Ignored,
            _ => return Outcome::Ignored,
        };
        Outcome::Message(message)
    }
}

fn kind_color(kind: crate::app::Kind) -> Color {
    match kind {
        crate::app::Kind::Dir => Color::Rgb(120, 190, 250),
        crate::app::Kind::Code => Color::Rgb(190, 150, 240),
        crate::app::Kind::Text => Color::Rgb(190, 214, 140),
        crate::app::Kind::Image => Color::Rgb(240, 160, 140),
        crate::app::Kind::Audio | crate::app::Kind::Video => Color::Rgb(230, 180, 120),
        crate::app::Kind::Archive | crate::app::Kind::Binary => Color::Rgb(200, 150, 110),
        crate::app::Kind::Other => Color::Rgb(168, 178, 194),
    }
}

fn truncate(value: &str, max: usize) -> String {
    let mut chars = value.chars();
    let mut output: String = chars.by_ref().take(max).collect();
    if chars.next().is_some() {
        output.push('…');
    }
    output
}

#[cfg(test)]
mod tests {
    use ferriso_core::App;
    use ferriso_tui::ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::app::Yazi;

    #[test]
    fn tui_draws_the_actor_snapshot() {
        let app = Yazi::new(std::env::temp_dir(), false);
        let view = app.view();
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("terminal");
        let mut tui = YaziTui::new();
        terminal
            .draw(|frame| tui.draw(&view, frame))
            .expect("draw yazi");
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(text.contains("YAZI"));
        assert!(text.contains("move"));
    }

    #[test]
    fn navigation_keys_are_semantic_messages() {
        let app = Yazi::new(std::env::temp_dir(), false);
        let view = app.view();
        let mut tui = YaziTui::new();
        let outcome = tui.handle(
            &view,
            Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
        );
        assert!(matches!(
            outcome,
            Outcome::Message(Message::MoveSelection(1))
        ));
    }

    #[test]
    fn enter_closes_the_filter_without_activating_the_selection() {
        let app = Yazi::new(std::env::temp_dir(), false);
        let view = app.view();
        let mut tui = YaziTui::new();
        tui.start_filter(&view);
        let outcome = tui.handle(
            &view,
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        );
        assert!(matches!(outcome, Outcome::Ignored));
        assert!(tui.filter.is_none());
    }

    #[test]
    fn filter_typing_stays_local_until_a_semantic_draft_is_sent() {
        let app = Yazi::new(std::env::temp_dir(), false);
        let view = app.view();
        let mut tui = YaziTui::new();
        tui.start_filter(&view);
        let outcome = tui.handle(
            &view,
            Event::Key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE)),
        );
        assert!(matches!(
            outcome,
            Outcome::Message(Message::FilterChanged(ref value)) if value == "r"
        ));
        assert_eq!(tui.filter.as_deref(), Some("r"));
    }
}
