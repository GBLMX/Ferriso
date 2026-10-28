//! A tiny terminal art demo: aurora backdrop, gradient bar, pulsing border,
//! and a moving shimmer title.
//!
//! Run with `cargo run -p ferriso-tui --example art`; `q` or `Esc` exits.

use std::time::Duration;

use ferriso_core::{App, Command, Subscription};
use ferriso_tui::{
    Outcome, Renderer,
    art::{Aurora, Gradient, Palette, Pulse, Shimmer, grapheme_cells},
    crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers},
    ratatui::{
        Frame,
        layout::Rect,
        style::{Color, Modifier, Style},
        text::{Line, Span},
        widgets::{Block, Paragraph},
    },
};
use ratatui::widgets::Wrap;

const TICK: Duration = Duration::from_millis(50);

struct Demo {
    tick: u64,
}

#[derive(Clone)]
enum Message {
    Tick,
}

impl App for Demo {
    type Message = Message;
    type View = u64;

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::Tick => self.tick = self.tick.wrapping_add(1),
        }
        Command::none()
    }

    fn view(&self) -> u64 {
        self.tick
    }

    fn subscriptions(&self) -> Subscription<Message> {
        Subscription::interval(1, TICK, Message::Tick)
    }
}

struct ArtRenderer;

impl Renderer<u64, Message> for ArtRenderer {
    fn draw(&mut self, tick: &u64, frame: &mut Frame) {
        let time = *tick as f32 * TICK.as_secs_f32();
        let area = frame.area();
        let palette = Palette::EMBER;

        Aurora::new(time, palette)
            .intensity(0.72)
            .render(area, frame.buffer_mut());

        let bar = Rect::new(area.x, area.y, area.width, area.height.min(3));
        Gradient::horizontal(palette.abyss, palette.glow).paint(bar, frame.buffer_mut());

        let pulse = Pulse {
            period: 2.2,
            min: 0.25,
            max: 1.0,
        };
        let border = Style::default()
            .fg(pulse.color(time, palette.glow, palette.highlight))
            .add_modifier(Modifier::BOLD);
        let block = Block::bordered()
            .border_style(border)
            .title(" ferriso-tui::art ")
            .title_style(Style::default().fg(palette.highlight));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let shimmer = Shimmer {
            period: 3.0,
            width: 5.0,
            base: palette.accent,
            highlight: palette.highlight,
        };
        let title = "ADVANCED TERMINAL ART";
        let title_width = grapheme_cells(title).map(|(_, width)| width).sum();
        let mut column = 0usize;
        let title = Line::from(
            grapheme_cells(title)
                .map(|(cluster, width)| {
                    let span = Span::styled(
                        cluster,
                        Style::default().fg(shimmer.color_at(column, title_width, time)),
                    );
                    column += width;
                    span
                })
                .collect::<Vec<_>>(),
        );
        let body = Paragraph::new(vec![
            title,
            Line::default(),
            Line::from(Span::styled(
                "aurora + gradient + pulse + shimmer",
                Style::default().fg(Color::Rgb(235, 230, 220)),
            )),
            Line::from(Span::styled(
                "q / Esc exits",
                Style::default().fg(palette.highlight),
            )),
        ])
        .wrap(Wrap { trim: true });
        frame.render_widget(body, inner);
    }

    fn handle(&mut self, _view: &u64, event: Event) -> Outcome<Message> {
        match event {
            Event::Key(KeyEvent {
                code: KeyCode::Char('q'),
                modifiers: KeyModifiers::NONE,
                ..
            })
            | Event::Key(KeyEvent {
                code: KeyCode::Esc, ..
            }) => Outcome::Quit,
            _ => Outcome::Ignored,
        }
    }
}

fn main() -> std::io::Result<()> {
    ferriso_tui::run(
        Demo { tick: 0 },
        ArtRenderer,
        ferriso_tui::Options::default(),
    )
}
