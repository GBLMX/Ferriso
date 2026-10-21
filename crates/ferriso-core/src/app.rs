//! The application contract owned by the core actor.
//!
//! An [`App`] is plain domain state. It knows nothing about terminals, windows,
//! widgets, or layout. Frontends translate raw input into semantic messages and
//! render the [`App::View`] snapshot with their own native layout.

use std::time::Duration;

use crate::Command;

/// A headless application driven by [`crate::spawn`].
///
/// The state lives inside the actor task. No frontend ever owns it or calls
/// [`App::update`] directly; frontends send messages through a
/// [`crate::CoreHandle`] and receive immutable [`App::View`] snapshots.
pub trait App: Send + 'static {
    /// Semantic input understood by this application.
    ///
    /// This type must not contain terminal or GUI toolkit types. Frontends are
    /// responsible for mapping keys, clicks, and text edits onto it.
    type Message: Send + 'static;

    /// Frontend-neutral snapshot of everything a frontend needs to draw.
    ///
    /// The snapshot is deliberately not a widget tree: each frontend chooses
    /// its own layout from this data.
    type View: Send + Sync + 'static;

    /// Initialize state and return commands to run once the actor starts.
    ///
    /// Called synchronously by [`crate::spawn`] before the first snapshot, so
    /// the frontend never observes a pre-initialization view.
    fn init(&mut self) -> Command<Self::Message> {
        Command::none()
    }

    /// Apply one semantic message and return follow-up commands.
    fn update(&mut self, message: Self::Message) -> Command<Self::Message>;

    /// Build the current view data. Called only on the actor task.
    fn view(&self) -> Self::View;

    /// How often [`App::tick`] should run, if at all.
    fn tick_interval(&self) -> Option<Duration> {
        None
    }

    /// Handle one timer tick. The actor owns the timer.
    fn tick(&mut self) -> Command<Self::Message> {
        Command::none()
    }

    /// Whether the actor should stop after the current message or tick.
    fn should_quit(&self) -> bool {
        false
    }

    /// Last chance to release resources before the actor exits.
    fn shutdown(&mut self) {}
}
