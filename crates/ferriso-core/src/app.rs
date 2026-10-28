//! The application contract owned by the core actor.
//!
//! An [`App`] is plain domain state. It knows nothing about terminals, windows,
//! widgets, or layout. Frontends translate raw input into semantic messages and
//! render the [`App::View`] snapshot with their own native layout.

use std::{future::Future, pin::Pin};

use crate::{Command, Subscription};

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

    /// Declare the long-lived streams that should be active for this state.
    ///
    /// Called after `init` and every update. Streams are keyed;
    /// keys that disappear are aborted, while existing keys keep running.
    /// Return a stable key per logical source and change it when the stream's
    /// parameters change.
    fn subscriptions(&self) -> Subscription<Self::Message> {
        Subscription::none()
    }

    /// Whether the actor should stop after the current message.
    fn should_quit(&self) -> bool {
        false
    }

    /// Describe one message for the debug tap.
    ///
    /// Called only by [`crate::spawn_with_debug`], on the actor task, before
    /// the message is applied. Return `None` to fall back to the message type
    /// name. Keep this cheap; it is not part of normal operation.
    fn debug(&self, _message: &Self::Message) -> Option<String> {
        None
    }

    /// Flush asynchronous resources during shutdown.
    ///
    /// The actor awaits this future after aborting in-flight commands and
    /// subscriptions, before calling [`App::shutdown`]. Return `None` when no
    /// async flush is needed. Implementations should bound their own work;
    /// the runtime does not impose a timeout, though adapter-owned `run`
    /// conveniences may bound how long they wait for the actor to stop.
    fn shutdown_async(&mut self) -> Option<Pin<Box<dyn Future<Output = ()> + Send + '_>>> {
        None
    }

    /// Last chance to release resources synchronously before the actor exits.
    ///
    /// Runs after [`App::shutdown_async`] completes. Resources that require
    /// an async runtime must be released in `shutdown_async`; this hook is for
    /// synchronous cleanup and final logging.
    fn shutdown(&mut self) {}
}
