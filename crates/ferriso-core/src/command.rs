//! Renderer-independent asynchronous work.
//!
//! A [`Command`] is a set of futures owned by the core actor. The actor spawns
//! them on its Tokio runtime and feeds their results back as ordinary messages.

use std::{collections::HashSet, future::Future, pin::Pin};

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
    /// key, only the last entry is spawned. Earlier entries are dropped before
    /// the actor sees them, so an immediately-ready future cannot deliver a
    /// result in the gap before its replacement is dispatched.
    pub fn batch(commands: impl IntoIterator<Item = Self>) -> Self {
        let mut futures: Vec<_> = commands
            .into_iter()
            .flat_map(|command| command.futures)
            .collect();
        let mut seen = HashSet::new();
        futures.reverse();
        futures.retain(|entry| match entry.key {
            Some(key) => seen.insert(key),
            None => true,
        });
        futures.reverse();
        Self { futures }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(command: Command<()>) -> Vec<Option<u64>> {
        command.into_futures().map(|(key, _)| key).collect()
    }

    #[test]
    fn batch_spawns_only_the_last_command_for_a_key() {
        let batch = Command::batch([
            Command::perform_keyed(1, async {}, |_| ()),
            Command::perform(async {}, |_| ()),
            Command::perform_keyed(1, async {}, |_| ()),
            Command::perform_keyed(2, async {}, |_| ()),
        ]);

        assert_eq!(keys(batch), vec![None, Some(1), Some(2)]);
    }

    #[test]
    fn nested_batches_deduplicate_keys_across_boundaries() {
        let first = Command::batch([
            Command::perform_keyed(7, async {}, |_| ()),
            Command::perform_keyed(7, async {}, |_| ()),
        ]);
        let second = Command::batch([
            Command::perform_keyed(8, async {}, |_| ()),
            Command::perform_keyed(7, async {}, |_| ()),
        ]);

        assert_eq!(
            keys(Command::batch([first, second])),
            vec![Some(8), Some(7)]
        );
    }
}
