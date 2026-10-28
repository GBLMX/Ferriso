use std::{borrow::Cow, sync::Arc, time::Duration};

use ferriso_core::{App, Command, Subscription};
use rio_vt::{
    ansi::CursorShape,
    config::colors::{AnsiColor, NamedColor},
    crosswords::{Crosswords, CrosswordsSize, style::StyleFlags},
    event::{Msg, VoidListener, WindowId, WindowSize, sync::FairMutex},
    performer::Machine,
    teletypewriter,
};

pub const COLUMNS: usize = 100;
pub const ROWS: usize = 30;
const SCROLLBACK_LINES: usize = 10_000;
const FRAME_INTERVAL: Duration = Duration::from_millis(33);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellView {
    pub character: char,
    pub foreground: (u8, u8, u8),
    pub background: (u8, u8, u8),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalView {
    pub columns: usize,
    pub rows: Vec<Vec<CellView>>,
    pub cursor: Option<(usize, usize)>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Message {
    Tick,
    Input(Vec<u8>),
    Resize { columns: usize, rows: usize },
}

struct TerminalSession {
    terminal: Arc<FairMutex<Crosswords<VoidListener>>>,
    writer: Option<rio_vt::corcovado::channel::Sender<Msg>>,
    columns: usize,
    rows: usize,
    error: Option<String>,
}

impl TerminalSession {
    fn start() -> Self {
        let size = CrosswordsSize::new(COLUMNS, ROWS);
        let terminal = Arc::new(FairMutex::new(Crosswords::new(
            size,
            CursorShape::Block,
            VoidListener,
            WindowId::from(0),
            0,
            SCROLLBACK_LINES,
        )));

        let env = Some(vec![
            ("TERM".to_string(), "xterm-256color".to_string()),
            ("COLORTERM".to_string(), "truecolor".to_string()),
        ]);

        #[cfg(unix)]
        let pty = teletypewriter::create_pty_with_spawn(
            None,
            Vec::new(),
            &None,
            env,
            COLUMNS as u16,
            ROWS as u16,
            0,
            0,
        );
        #[cfg(windows)]
        let pty =
            teletypewriter::create_pty(None, Vec::new(), &None, env, COLUMNS as u16, ROWS as u16);

        let pty = match pty {
            Ok(pty) => pty,
            Err(error) => {
                return Self {
                    terminal,
                    writer: None,
                    columns: COLUMNS,
                    rows: ROWS,
                    error: Some(format!("could not start shell: {error}")),
                };
            }
        };

        let machine = match Machine::new(
            Arc::clone(&terminal),
            pty,
            VoidListener,
            WindowId::from(0),
            0,
        ) {
            Ok(machine) => machine,
            Err(error) => {
                return Self {
                    terminal,
                    writer: None,
                    columns: COLUMNS,
                    rows: ROWS,
                    error: Some(format!("could not start terminal reader: {error}")),
                };
            }
        };

        let writer = machine.channel();
        machine.spawn();

        Self {
            terminal,
            writer: Some(writer),
            columns: COLUMNS,
            rows: ROWS,
            error: None,
        }
    }

    fn resize(&mut self, columns: usize, rows: usize) {
        let columns = columns.max(1);
        let rows = rows.max(1);
        if columns == self.columns && rows == self.rows {
            return;
        }
        self.columns = columns;
        self.rows = rows;
        self.terminal
            .lock()
            .resize(CrosswordsSize::new(columns, rows));
        if let Some(writer) = &self.writer {
            let _ = writer.send(Msg::Resize(WindowSize {
                rows: rows as u16,
                cols: columns as u16,
                width: 0,
                height: 0,
            }));
        }
    }

    fn input(&self, bytes: Vec<u8>) {
        if let Some(writer) = &self.writer {
            let _ = writer.send(Msg::Input(Cow::Owned(bytes)));
        }
    }

    fn snapshot(&self) -> TerminalView {
        if let Some(error) = &self.error {
            return TerminalView {
                columns: self.columns,
                rows: Vec::new(),
                cursor: None,
                error: Some(error.clone()),
            };
        }

        let terminal = self.terminal.lock();
        let visible = terminal.visible_rows();
        let rows = visible
            .iter()
            .map(|row| {
                (0..self.columns)
                    .map(|column| {
                        let cell = row.inner.get(column).copied().unwrap_or_default();
                        let style = terminal.grid.style_of(&cell);
                        let (mut foreground, mut background) =
                            (style_color(style.fg), style_color(style.bg));

                        if style.flags.contains(StyleFlags::INVERSE) {
                            std::mem::swap(&mut foreground, &mut background);
                        }

                        CellView {
                            character: cell.c(),
                            foreground,
                            background,
                        }
                    })
                    .collect()
            })
            .collect();

        let cursor = terminal.grid.cursor.pos;
        let cursor = (cursor.row.0 >= 0
            && (cursor.row.0 as usize) < self.rows
            && cursor.col.0 < self.columns)
            .then_some((cursor.row.0 as usize, cursor.col.0));

        TerminalView {
            columns: self.columns,
            rows,
            cursor,
            error: None,
        }
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        if let Some(writer) = &self.writer {
            let _ = writer.send(Msg::Shutdown);
        }
    }
}

pub struct TerminalApp {
    session: TerminalSession,
}

impl TerminalApp {
    pub fn new() -> Self {
        Self {
            session: TerminalSession::start(),
        }
    }
}

impl Default for TerminalApp {
    fn default() -> Self {
        Self::new()
    }
}

impl App for TerminalApp {
    type Message = Message;
    type View = TerminalView;

    fn update(&mut self, message: Self::Message) -> Command<Self::Message> {
        match message {
            Message::Tick => {}
            Message::Input(bytes) => self.session.input(bytes),
            Message::Resize { columns, rows } => self.session.resize(columns, rows),
        }
        Command::none()
    }

    fn view(&self) -> Self::View {
        self.session.snapshot()
    }

    fn subscriptions(&self) -> Subscription<Self::Message> {
        Subscription::interval(1, FRAME_INTERVAL, Message::Tick)
    }

    fn shutdown(&mut self) {
        if let Some(writer) = &self.session.writer {
            let _ = writer.send(Msg::Shutdown);
        }
    }
}

fn style_color(color: AnsiColor) -> (u8, u8, u8) {
    match color {
        AnsiColor::Spec(color) => (color.r, color.g, color.b),
        AnsiColor::Indexed(index) => indexed_color(index),
        AnsiColor::Named(name) => named_color(name),
    }
}

fn named_color(color: NamedColor) -> (u8, u8, u8) {
    match color {
        NamedColor::Black => (0x1e, 0x1e, 0x2e),
        NamedColor::Red => (0xf3, 0x8b, 0xa8),
        NamedColor::Green => (0xa6, 0xe3, 0xa1),
        NamedColor::Yellow => (0xf9, 0xe2, 0xaf),
        NamedColor::Blue => (0x89, 0xb4, 0xfa),
        NamedColor::Magenta => (0xcb, 0xa6, 0xf7),
        NamedColor::Cyan => (0x89, 0xdc, 0xeb),
        NamedColor::White => (0xba, 0xcb, 0xf2),
        NamedColor::LightBlack | NamedColor::DimBlack => (0x58, 0x5b, 0x70),
        NamedColor::LightRed | NamedColor::DimRed => (0xf3, 0x8b, 0xa8),
        NamedColor::LightGreen | NamedColor::DimGreen => (0xa6, 0xe3, 0xa1),
        NamedColor::LightYellow | NamedColor::DimYellow => (0xf9, 0xe2, 0xaf),
        NamedColor::LightBlue | NamedColor::DimBlue => (0x89, 0xb4, 0xfa),
        NamedColor::LightMagenta | NamedColor::DimMagenta => (0xcb, 0xa6, 0xf7),
        NamedColor::LightCyan | NamedColor::DimCyan => (0x89, 0xdc, 0xeb),
        NamedColor::LightWhite | NamedColor::DimWhite => (0xba, 0xcb, 0xf2),
        NamedColor::Foreground | NamedColor::LightForeground => (0xcd, 0xd6, 0xf4),
        NamedColor::DimForeground => (0x7f, 0x84, 0x9c),
        NamedColor::Background => (0x1e, 0x1e, 0x2e),
        NamedColor::Cursor => (0xf5, 0xe0, 0xdc),
    }
}

fn indexed_color(index: u8) -> (u8, u8, u8) {
    match index {
        0..=15 => named_color(match index {
            0 => NamedColor::Black,
            1 => NamedColor::Red,
            2 => NamedColor::Green,
            3 => NamedColor::Yellow,
            4 => NamedColor::Blue,
            5 => NamedColor::Magenta,
            6 => NamedColor::Cyan,
            7 => NamedColor::White,
            8 => NamedColor::LightBlack,
            9 => NamedColor::LightRed,
            10 => NamedColor::LightGreen,
            11 => NamedColor::LightYellow,
            12 => NamedColor::LightBlue,
            13 => NamedColor::LightMagenta,
            14 => NamedColor::LightCyan,
            _ => NamedColor::LightWhite,
        }),
        16..=231 => {
            let value = index - 16;
            let r = value / 36;
            let g = (value % 36) / 6;
            let b = value % 6;
            (
                if r == 0 { 0 } else { r * 40 + 55 },
                if g == 0 { 0 } else { g * 40 + 55 },
                if b == 0 { 0 } else { b * 40 + 55 },
            )
        }
        _ => {
            let gray = 8 + (index - 232) * 10;
            (gray, gray, gray)
        }
    }
}
