//! Renderer-independent asynchronous work.
//!
//! A [`Command`] is a set of futures owned by the core actor. The actor spawns
//! them on its Tokio runtime and feeds their results back as ordinary messages.

use std::future::Future;
use std::pin::Pin;

type BoxFuture<M> = Pin<Box<dyn Future<Output = M> + Send + 'static>>;

/// Deferred work that produces zero or more messages.
pub struct Command<M> {
    futures: Vec<BoxFuture<M>>,
}

impl<M> Command<M> {
    /// No work.
    pub fn none() -> Self {
        Self {
            futures: Vec::new(),
        }
    }

    /// Run `future`, then turn its output into one message with `map`.
    pub fn perform<Fut, T>(future: Fut, map: impl FnOnce(T) -> M + Send + 'static) -> Self
    where
        Fut: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        Self {
            futures: vec![Box::pin(async move { map(future.await) })],
        }
    }

    /// Run every command in `commands` concurrently.
    pub fn batch(commands: impl IntoIterator<Item = Self>) -> Self {
        Self {
            futures: commands
                .into_iter()
                .flat_map(|command| command.futures)
                .collect(),
        }
    }

    pub(crate) fn into_futures(self) -> impl Iterator<Item = BoxFuture<M>> {
        self.futures.into_iter()
    }
}

impl<M> Default for Command<M> {
    fn default() -> Self {
        Self::none()
    }
}
