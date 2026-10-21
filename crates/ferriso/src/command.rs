//! Asynchronous work returned by [`App`](crate::App) callbacks.
//!
//! A command is renderer-neutral: it owns a boxed future producing a message. The renderer
//! decides where to run it (iced's executor, or a Tokio runtime in the terminal loop), so the
//! application never names an executor.

use std::{future::Future, pin::Pin};

pub type BoxFuture<Message> = Pin<Box<dyn Future<Output = Message> + Send + 'static>>;

/// Deferred messages. `Command::none()` is the common answer to a message that only mutates
/// state; anything that must wait returns one or more futures.
pub struct Command<Message> {
    futures: Vec<BoxFuture<Message>>,
}

impl<Message> Default for Command<Message> {
    fn default() -> Self {
        Self::none()
    }
}

impl<Message> Command<Message> {
    pub fn none() -> Self {
        Self {
            futures: Vec::new(),
        }
    }

    /// Deliver a message immediately (on the next event-loop turn).
    pub fn message(message: Message) -> Self
    where
        Message: Send + 'static,
    {
        Self::perform(std::future::ready(()), move |()| message)
    }

    /// Run `future` and feed its result through `map`. The future and the mapper must be
    /// `Send`: a renderer is free to run them on a worker thread.
    pub fn perform<F, T>(future: F, map: impl FnOnce(T) -> Message + Send + 'static) -> Self
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
        Message: Send + 'static,
    {
        Self {
            futures: vec![Box::pin(async move { map(future.await) })],
        }
    }

    pub fn batch(commands: impl IntoIterator<Item = Self>) -> Self {
        Self {
            futures: commands
                .into_iter()
                .flat_map(|command| command.futures)
                .collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.futures.is_empty()
    }

    pub fn len(&self) -> usize {
        self.futures.len()
    }

    /// Used by renderers. Consuming the command prevents a future from being polled twice.
    pub fn into_futures(self) -> impl Iterator<Item = BoxFuture<Message>> {
        self.futures.into_iter()
    }
}

impl<Message> From<Message> for Command<Message>
where
    Message: Send + 'static,
{
    fn from(message: Message) -> Self {
        Self::message(message)
    }
}
