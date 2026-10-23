//! Adapter conformance: one actor, one test application, both frontends.
//!
//! The test drives the real `Renderer` translation surfaces. It does not open
//! a terminal or an iced window; those live-window checks stay in the
//! compatibility script.

use std::time::Duration;

use ferriso_core::{App, Command};
use ferriso_gui::{Message as GuiMessage, Renderer as GuiRenderer};
use ferriso_tui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ferriso_tui::ratatui::{Terminal, backend::TestBackend, widgets::Paragraph};
use ferriso_tui::{Outcome, Renderer as TuiRenderer};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Message {
    Add,
}

struct Counter(u32);

impl App for Counter {
    type Message = Message;
    type View = u32;

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::Add => self.0 += 1,
        }
        Command::none()
    }

    fn view(&self) -> u32 {
        self.0
    }
}

#[derive(Default)]
struct TuiCounter {
    seen: Vec<u32>,
}

impl TuiRenderer<u32, Message> for TuiCounter {
    fn draw(&mut self, view: &u32, frame: &mut ferriso_tui::ratatui::Frame) {
        frame.render_widget(Paragraph::new(view.to_string()), frame.area());
    }

    fn handle(&mut self, _view: &u32, event: Event) -> Outcome<Message> {
        match event {
            Event::Key(KeyEvent {
                code: KeyCode::Enter,
                ..
            }) => Outcome::Message(Message::Add),
            _ => Outcome::Ignored,
        }
    }

    fn on_snapshot(&mut self, view: &u32) {
        self.seen.push(*view);
    }
}

#[derive(Default)]
struct GuiCounter {
    seen: Vec<u32>,
}

impl GuiRenderer<u32, Message, ()> for GuiCounter {
    fn view<'a>(
        &'a self,
        view: &'a u32,
    ) -> ferriso_gui::iced::Element<'a, GuiMessage<u32, Message, ()>> {
        ferriso_gui::iced::widget::text(view.to_string()).into()
    }

    fn on_snapshot(&mut self, view: &u32) {
        self.seen.push(*view);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn one_actor_feeds_both_adapters_with_the_same_semantics() {
    let (handle, snapshots, task) = ferriso_core::spawn(Counter(0));
    let mut tui_snapshots = snapshots;
    let mut gui_snapshots = tui_snapshots.clone();

    let boot = tui_snapshots.current();
    assert_eq!(*boot, 0);

    let mut tui = TuiCounter::default();
    let mut gui = GuiCounter::default();
    tui.on_snapshot(&boot);
    gui.on_snapshot(&boot);

    let mut terminal = Terminal::new(TestBackend::new(20, 1)).expect("test terminal");
    terminal
        .draw(|frame| tui.draw(&boot, frame))
        .expect("tui draws boot snapshot");
    drop(gui.view(&boot));

    let event = Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    match tui.handle(&boot, event) {
        Outcome::Message(message) => assert!(handle.send(message)),
        Outcome::Ignored | Outcome::Quit => panic!("Enter must produce a semantic message"),
    }

    let tui_view = tokio::time::timeout(Duration::from_secs(1), tui_snapshots.changed())
        .await
        .expect("tui snapshot arrives")
        .expect("actor alive");
    let gui_view = tokio::time::timeout(Duration::from_secs(1), gui_snapshots.changed())
        .await
        .expect("gui snapshot arrives")
        .expect("actor alive");
    assert_eq!(*tui_view, 1);
    assert_eq!(*gui_view, 1);

    tui.on_snapshot(&tui_view);
    gui.on_snapshot(&gui_view);
    terminal
        .draw(|frame| tui.draw(&tui_view, frame))
        .expect("tui draws updated snapshot");
    drop(gui.view(&gui_view));

    let text: String = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(text.contains('1'), "{text:?}");
    assert_eq!(tui.seen, vec![0, 1]);
    assert_eq!(gui.seen, vec![0, 1]);

    handle.shutdown();
    tokio::time::timeout(Duration::from_secs(1), task.join())
        .await
        .expect("actor exits");
    assert!(handle.is_closed());
    assert!(tui_snapshots.closed());
    assert!(gui_snapshots.closed());
}
