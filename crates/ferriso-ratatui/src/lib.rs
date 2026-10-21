//! Ratatui backend for Ferriso.
//!
//! `run` owns the terminal, the event loop, and a Tokio runtime for commands. It draws the
//! same [`ferriso::Node`] tree an iced window would draw, with terminal behaviour for focus,
//! text editing, list selection, and scrolling.

mod input;
mod layout;
mod render;

use std::{
    io,
    sync::mpsc,
    time::{Duration, Instant},
};

use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event};
use ferriso::{App, Command};

pub use render::{Hit, HitKind, UiState};

use input::{handle_key, handle_mouse, map_key};

#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// Capture mouse clicks and wheel events. Keyboard-only use can turn this off.
    pub mouse: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self { mouse: true }
    }
}

/// Run an application on the terminal until it quits.
pub fn run<A: App>(mut app: A, options: Options) -> io::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let (sender, receiver) = mpsc::channel::<A::Message>();
    let mut state = UiState::new();
    let mut next_tick = app.tick_interval().map(|interval| {
        Instant::now() + interval.max(Duration::from_millis(16))
    });
    let mut interval = app.tick_interval();

    spawn(&runtime, &sender, app.init());

    let mut terminal = ratatui::init();
    if options.mouse {
        crossterm::execute!(io::stdout(), EnableMouseCapture)?;
    }

    let result = (|| -> io::Result<()> {
        loop {
            let theme = app.theme();
            terminal.draw(|frame| {
                let area = frame.area();
                let view = app.view();
                render::draw(frame.buffer_mut(), area, &view, &theme, &mut state);
            })?;

            let timeout = next_tick
                .map(|deadline| deadline.saturating_duration_since(Instant::now()))
                .unwrap_or(Duration::from_millis(50))
                .min(Duration::from_millis(50));
            if event::poll(timeout)? {
                match event::read()? {
                    Event::Key(key) => {
                        let command = handle_key(&mut app, &mut state, map_key(key));
                        spawn(&runtime, &sender, command);
                    }
                    Event::Mouse(mouse) => {
                        let command = handle_mouse(&mut app, &mut state, mouse);
                        spawn(&runtime, &sender, command);
                    }
                    Event::Paste(text) => {
                        let event = ferriso::KeyEvent {
                            key: ferriso::Key::Character(text.clone()),
                            modifiers: ferriso::Modifiers::NONE,
                            pressed: true,
                            repeat: false,
                            text: Some(text),
                        };
                        let command = handle_key(&mut app, &mut state, event);
                        spawn(&runtime, &sender, command);
                    }
                    Event::Resize(_, _) => {
                        state.list_offsets.clear();
                    }
                    _ => {}
                }
            }

            while let Ok(message) = receiver.try_recv() {
                let command = app.update(message);
                spawn(&runtime, &sender, command);
            }

            let current_interval = app.tick_interval();
            if current_interval != interval {
                interval = current_interval;
                next_tick = interval.map(|_| Instant::now());
            }
            if let Some(deadline) = next_tick
                && Instant::now() >= deadline
            {
                let command = app.tick();
                spawn(&runtime, &sender, command);
                next_tick = interval.map(|interval| {
                    Instant::now() + interval.max(Duration::from_millis(16))
                });
            }

            if state.quit || app.should_quit() {
                break;
            }
        }
        Ok(())
    })();

    if options.mouse {
        let _ = crossterm::execute!(io::stdout(), DisableMouseCapture);
    }
    ratatui::restore();
    result
}

fn spawn<Message: Send + 'static>(
    runtime: &tokio::runtime::Runtime,
    sender: &mpsc::Sender<Message>,
    command: Command<Message>,
) {
    for future in command.into_futures() {
        let sender = sender.clone();
        runtime.spawn(async move {
            let _ = sender.send(future.await);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent as CrosstermKey, KeyModifiers};
    use ferriso::prelude::*;
    use ratatui::{Terminal, backend::TestBackend};

    #[derive(Clone, Debug)]
    enum Msg {
        Input(String),
        Select(usize),
        Activate(usize),
        Pressed,
    }

    struct Fixture {
        value: String,
        selected: Option<usize>,
        activated: Option<usize>,
        pressed: bool,
        wide: bool,
    }

    impl Fixture {
        fn new() -> Self {
            Self {
                value: String::new(),
                selected: None,
                activated: None,
                pressed: false,
                wide: false,
            }
        }
    }

    impl App for Fixture {
        type Message = Msg;

        fn update(&mut self, message: Msg) -> Command<Msg> {
            match message {
                Msg::Input(value) => self.value = value,
                Msg::Select(index) => self.selected = Some(index),
                Msg::Activate(index) => self.activated = Some(index),
                Msg::Pressed => self.pressed = true,
            }
            Command::none()
        }

        fn view(&self) -> Node<'_, Msg> {
            if self.wide {
                return row()
                    .height(Length::Fill)
                    .push(
                        column()
                            .width(Length::Fixed(80))
                            .height(Length::Fill)
                            .padding(Padding::all(4))
                            .push(text("SIDEBAR")),
                    )
                    .push(text("CONTENT"))
                    .into();
            }
            let items = vec![
                ListItem::new("first"),
                ListItem::new("second"),
                ListItem::new("third"),
            ];
            column()
                .padding(Padding::all(8))
                .spacing(4)
                .push(
                    TextInput::new(self.value.as_str())
                        .placeholder("type")
                        .on_input(Msg::Input),
                )
                .push(button("press", Msg::Pressed))
                .push(
                    List::new(items)
                        .selected(self.selected)
                        .on_select(Msg::Select)
                        .on_activate(Msg::Activate)
                        .height(Length::Fixed(80)),
                )
                .into()
        }
    }

    fn key(code: KeyCode) -> KeyEvent {
        map_key(CrosstermKey::new(code, KeyModifiers::NONE))
    }

    fn draw(app: &Fixture, state: &mut UiState) -> Vec<String> {
        let backend = TestBackend::new(40, 14);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                let theme = app.theme();
                let view = app.view();
                render::draw(frame.buffer_mut(), area, &view, &theme, state);
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect()
    }

    #[test]
    fn fixed_widths_are_scaled_to_cells() {
        let mut app = Fixture::new();
        app.wide = true;
        let mut state = UiState::new();
        let screen = draw(&app, &mut state);
        assert!(screen[1].starts_with(" SIDEBAR"), "{:?}", screen[1]);
        assert!(screen[0].starts_with("          CONTENT"), "{:?}", screen[0]);
    }

    #[test]
    fn tab_focuses_the_input_and_typing_reaches_the_model() {
        let mut app = Fixture::new();
        let mut state = UiState::new();
        draw(&app, &mut state);
        handle_key(&mut app, &mut state, key(KeyCode::Tab));
        draw(&app, &mut state);
        // first focusable is the input in this fixture
        handle_key(&mut app, &mut state, key(KeyCode::Char('h')));
        handle_key(&mut app, &mut state, key(KeyCode::Char('i')));
        assert_eq!(app.value, "hi");
        let screen = draw(&app, &mut state);
        assert!(screen.iter().any(|line| line.contains("hi")), "{screen:?}");
    }

    #[test]
    fn arrows_move_selection_and_enter_activates_it() {
        let mut app = Fixture::new();
        let mut state = UiState::new();
        let _ = draw(&app, &mut state);
        // input, button, list
        for _ in 0..3 {
            handle_key(&mut app, &mut state, key(KeyCode::Tab));
        }
        let _ = draw(&app, &mut state);
        handle_key(&mut app, &mut state, key(KeyCode::Down));
        assert_eq!(app.selected, Some(1));
        handle_key(&mut app, &mut state, key(KeyCode::Enter));
        assert_eq!(app.activated, Some(1));
    }

    #[test]
    fn shortcut_keys_go_to_the_application() {
        let mut app = Fixture::new();
        let mut state = UiState::new();
        let _ = draw(&app, &mut state);
        let mut event = key(KeyCode::Char(' '));
        event.modifiers.control = true;
        handle_key(&mut app, &mut state, event);
        assert!(!app.pressed);
    }
}
