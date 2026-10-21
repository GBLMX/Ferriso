//! The application contract shared by every backend.

use std::time::Duration;

use crate::{
    command::Command,
    key::KeyEvent,
    node::Node,
    theme::Theme,
};

/// A complete UI application, independent of where it is rendered.
///
/// The reducer owns all application state. Renderers call [`init`](App::init) once,
/// [`update`](App::update) for every message, and rebuild [`view`](App::view) on every frame.
/// Terminal navigation (focus, text cursor, scrolling) lives in the terminal renderer; a GUI
/// backend uses the native widgets' own behaviour. Neither is allowed to change application
/// state without going through a message.
pub trait App: Sized + 'static {
    type Message: Clone + Send + 'static;

    /// One-time work after construction. Both renderers call this before the first frame.
    fn init(&mut self) -> Command<Self::Message> {
        Command::none()
    }

    fn update(&mut self, message: Self::Message) -> Command<Self::Message>;

    fn view(&self) -> Node<'_, Self::Message>;

    fn theme(&self) -> Theme {
        Theme::dark()
    }

    fn title(&self) -> &str {
        "Ferriso"
    }

    /// Initial window size, in logical pixels. Ignored by the terminal backend.
    fn window_size(&self) -> (u32, u32) {
        (1180, 780)
    }

    /// How often [`tick`](App::tick) should run. `None` means no timer subscription.
    fn tick_interval(&self) -> Option<Duration> {
        None
    }

    /// Called on the subscription timer. Poll playback channels, advance animations, and
    /// return commands for anything that must wait.
    fn tick(&mut self) -> Command<Self::Message> {
        Command::none()
    }

    /// Key events the terminal renderer did not consume for focus navigation. GUI backends
    /// forward every key, because native widgets handle their own text input.
    fn on_key(&mut self, _event: KeyEvent) -> Command<Self::Message> {
        Command::none()
    }

    /// Polled after every message. Closes the window in the GUI; exits the loop in the TUI.
    fn should_quit(&self) -> bool {
        false
    }
}
