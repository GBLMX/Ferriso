use ferriso_tui::{
    Outcome, Renderer,
    crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers},
    ratatui::{
        Frame,
        layout::{Constraint, Layout},
        style::{Color, Modifier, Style},
        widgets::{Block, Cell, Paragraph, Row as TableRow, Table},
    },
};

use crate::app::{Message, WorkbenchView};

pub struct DataWorkbenchTui;

impl DataWorkbenchTui {
    pub const fn new() -> Self {
        Self
    }
}

impl Default for DataWorkbenchTui {
    fn default() -> Self {
        Self::new()
    }
}

impl Renderer<WorkbenchView, Message> for DataWorkbenchTui {
    fn draw(&mut self, view: &WorkbenchView, frame: &mut Frame) {
        let area = frame.area();
        let chunks = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(2),
        ])
        .split(area);

        frame.render_widget(
            Paragraph::new(format!(
                "Filter: {}   {}",
                if view.query.is_empty() {
                    "<type to filter>"
                } else {
                    view.query.as_str()
                },
                view.status
            ))
            .block(Block::bordered().title("DATA WORKBENCH")),
            chunks[0],
        );

        let header = TableRow::new([
            Cell::from("ID"),
            Cell::from("TITLE"),
            Cell::from("AUTHOR"),
            Cell::from("KIND"),
            Cell::from("STATUS"),
            Cell::from("DOWNLOADS"),
            Cell::from("UPDATED"),
        ])
        .style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
        let rows = view.rows.iter().map(|row| {
            let style = if row.selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            TableRow::new([
                Cell::from(format!("{:05}", row.id)),
                Cell::from(row.title.clone()),
                Cell::from(row.author.clone()),
                Cell::from(row.kind.clone()),
                Cell::from(row.status.clone()),
                Cell::from(format!("{:>9}", row.downloads)),
                Cell::from(row.updated.clone()),
            ])
            .style(style)
        });
        let table = Table::new(
            rows,
            [
                Constraint::Length(7),
                Constraint::Fill(3),
                Constraint::Length(14),
                Constraint::Length(14),
                Constraint::Length(11),
                Constraint::Length(10),
                Constraint::Length(10),
            ],
        )
        .header(header)
        .column_spacing(1);
        frame.render_widget(table, chunks[1]);

        frame.render_widget(
            Paragraph::new(format!(
                "↑/↓ move  PgUp/PgDn page  Home/End  Esc clear  Ctrl+Q quit   showing {}-{} / {}",
                view.visible_start, view.visible_end, view.filtered
            ))
            .style(Style::default().fg(Color::DarkGray)),
            chunks[2],
        );
    }

    fn handle(&mut self, view: &WorkbenchView, event: Event) -> Outcome<Message> {
        match event {
            Event::Resize(_, height) => {
                Outcome::Message(Message::Viewport(height.saturating_sub(6) as usize))
            }
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    && matches!(key.code, KeyCode::Char('q'))
                {
                    return Outcome::Quit;
                }
                match key.code {
                    KeyCode::Up => Outcome::Message(Message::Move(-1)),
                    KeyCode::Down => Outcome::Message(Message::Move(1)),
                    KeyCode::PageUp => Outcome::Message(Message::Page(-1)),
                    KeyCode::PageDown => Outcome::Message(Message::Page(1)),
                    KeyCode::Home => Outcome::Message(Message::First),
                    KeyCode::End => Outcome::Message(Message::Last),
                    KeyCode::Esc => Outcome::Message(Message::ClearFilter),
                    KeyCode::Backspace => {
                        let mut query = view.query.clone();
                        query.pop();
                        Outcome::Message(Message::QueryChanged(query))
                    }
                    KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                        let mut query = view.query.clone();
                        query.push(character);
                        Outcome::Message(Message::QueryChanged(query))
                    }
                    _ => Outcome::Ignored,
                }
            }
            _ => Outcome::Ignored,
        }
    }
}
