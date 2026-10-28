//! A minimal Unix-socket sidecar driven by the actor context.
//!
//! Run with `cargo run -p ferriso-tui --example sidecar`; connect to the printed
//! socket and send one line. The sidecar forwards each connection to the actor
//! as a semantic message and is removed when the terminal exits.

use std::{future::Future, io, path::PathBuf, pin::Pin, time::Duration};

use ferriso_core::{App, Command, CoreContext, Sidecar, Sidecars};
use ferriso_tui::{
    Outcome, Renderer,
    crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers},
    ratatui::{
        Frame,
        layout::Rect,
        widgets::{Paragraph, Wrap},
    },
};
use tokio::{io::AsyncWriteExt, net::UnixListener, task::JoinHandle};

struct Counter {
    pings: u32,
    socket: PathBuf,
}

#[derive(Clone)]
struct View {
    pings: u32,
    socket: PathBuf,
}

enum Message {
    Ping,
}

impl App for Counter {
    type Message = Message;
    type View = View;

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::Ping => self.pings += 1,
        }
        Command::none()
    }

    fn view(&self) -> View {
        View {
            pings: self.pings,
            socket: self.socket.clone(),
        }
    }
}

struct Terminal {
    socket: PathBuf,
}

impl Renderer<View, Message> for Terminal {
    fn draw(&mut self, view: &View, frame: &mut Frame) {
        let _ = self.socket.as_path();
        let text = format!(
            "Unix-sidecar dogfood\n\npings: {}\nsocket: {}\n\nsend a line with `nc -U {}`; q quits",
            view.pings,
            view.socket.display(),
            view.socket.display(),
        );
        frame.render_widget(
            Paragraph::new(text)
                .wrap(Wrap { trim: true })
                .block(ferriso_tui::ratatui::widgets::Block::bordered()),
            Rect::new(1, 1, frame.area().width.saturating_sub(2), 10),
        );
    }

    fn handle(&mut self, _view: &View, event: Event) -> Outcome<Message> {
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

struct SocketSidecar {
    task: JoinHandle<()>,
    path: PathBuf,
}

impl Sidecar for SocketSidecar {
    fn shutdown(self: Box<Self>) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        Box::pin(async move {
            let task = self.task;
            task.abort();
            let _ = task.await;
            let _ = std::fs::remove_file(self.path);
        })
    }
}

fn start_sidecar(context: CoreContext<Message, View>, path: PathBuf) -> Sidecars {
    let listener = UnixListener::bind(&path).expect("bind example sidecar socket");
    let handle = context.handle().clone();
    let task = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            if !handle.send(Message::Ping) {
                return;
            }
            let _ = stream.write_all(b"pong\n").await;
            let _ = stream.flush().await;
        }
    });

    let mut sidecars = Sidecars::new();
    sidecars.push(SocketSidecar {
        task,
        path: path.clone(),
    });
    sidecars
}

fn main() -> io::Result<()> {
    let socket = std::env::temp_dir().join(format!(
        "ferriso-sidecar-{}-{}.sock",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_nanos()
    ));
    let renderer_socket = socket.clone();
    let build_socket = socket.clone();

    ferriso_tui::run_with_app_services(
        move || Counter {
            pings: 0,
            socket: build_socket,
        },
        move |context| start_sidecar(context, socket),
        Terminal {
            socket: renderer_socket,
        },
        ferriso_tui::Options::default(),
    )
}
