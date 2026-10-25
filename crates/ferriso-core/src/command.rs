//! Renderer-independent asynchronous work.
//!
//! A [`Command`] is a set of futures owned by the core actor. The actor spawns
//! them on its Tokio runtime and feeds their results back as ordinary messages.

use std::future::Future;
use std::pin::Pin;

type BoxFuture<M> = Pin<Box<dyn Future<Output = M> + Send + 'static>>;

struct CommandFuture<M> {
    key: Option<u64>,
    future: BoxFuture<M>,
}

/// Deferred work that produces zero or more messages.
pub struct Command<M> {
    futures: Vec<CommandFuture<M>>,
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
            futures: vec![CommandFuture {
                key: None,
                future: Box::pin(async move { map(future.await) }),
            }],
        }
    }

    /// Run `future` as the latest-wins command for `key`.
    ///
    /// The actor permits at most one in-flight command per key. Dispatching a
    /// new keyed command aborts the previous future without emitting a message.
    /// A result that was already delivered is a normal message and is not
    /// retracted.
    ///
    /// Use this for replaceable work such as search-as-you-type. Do not use it
    /// for writes or installations that must complete even when the UI moves on;
    /// use [`CancellationToken`](crate::CancellationToken) when an aborted future
    /// needs cooperative cleanup.
    ///
    /// Keys are scoped to the actor and independent of subscription keys. If a
    /// batch contains the same key twice, the later command wins.
    ///
    /// ```no_run
    /// use std::time::Duration;
    ///
    /// use ferriso_core::Command;
    ///
    /// let latest_search: Command<u32> = Command::perform_keyed(
    ///     1,
    ///     async {
    ///         tokio::time::sleep(Duration::from_millis(250)).await;
    ///         42
    ///     },
    ///     |value| value,
    /// );
    /// # let _ = latest_search;
    /// ```
    pub fn perform_keyed<Fut, T>(
        key: u64,
        future: Fut,
        map: impl FnOnce(T) -> M + Send + 'static,
    ) -> Self
    where
        Fut: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        Self {
            futures: vec![CommandFuture {
                key: Some(key),
                future: Box::pin(async move { map(future.await) }),
            }],
        }
    }

    /// Run every command in `commands` concurrently.
    ///
    /// Keyed commands keep their keys; when two entries in the batch share a
    /// key, the later entry replaces the earlier one.
    pub fn batch(commands: impl IntoIterator<Item = Self>) -> Self {
        Self {
            futures: commands
                .into_iter()
                .flat_map(|command| command.futures)
                .collect(),
        }
    }

    pub(crate) fn into_futures(self) -> impl Iterator<Item = (Option<u64>, BoxFuture<M>)> {
        self.futures
            .into_iter()
            .map(|entry| (entry.key, entry.future))
    }
}

impl<M> Default for Command<M> {
    fn default() -> Self {
        Self::none()
    }
}
