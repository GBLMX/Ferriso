//! Long-lived, keyed event sources owned by the core actor.
//!
//! A [`Subscription`] is declarative: the application returns the streams that
//! should be active for its current state. The actor reconciles them after
//! `init` and every update:
//!
//! - new keys are spawned as background tasks;
//! - keys that are no longer returned are aborted;
//! - existing keys keep their running stream.
//!
//! The key is the identity of a stream. Change the key when the stream's
//! parameters change; keep it stable when only the view changes. Duplicate keys
//! in one [`Subscription`] are ignored, so callers should assign one key per
//! logical source.
//!
//! Use a subscription for long-lived sources such as file watchers, sockets,
//! animation clocks, and device events. Use [`crate::Command`] for one-shot
//! work. If a stream ends, its key remains active until the application stops
//! returning it; the actor does not silently restart it.

use std::{pin::Pin, time::Duration};

use futures::{Stream, StreamExt};
use tokio::time::{Instant, MissedTickBehavior, interval_at};

pub(crate) struct SubscriptionEntry<M> {
    pub(crate) key: u64,
    pub(crate) stream: Pin<Box<dyn Stream<Item = M> + Send + 'static>>,
}

/// A set of long-lived streams that produce application messages.
pub struct Subscription<M> {
    entries: Vec<SubscriptionEntry<M>>,
}

impl<M> Subscription<M> {
    /// No active streams.
    pub fn none() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Add one stream under a stable key.
    ///
    /// The key must uniquely identify the logical source for the lifetime of
    /// the application. Reusing a key keeps the already-running stream; it does
    /// not replace it with `stream`.
    pub fn stream<S>(key: u64, stream: S) -> Self
    where
        S: Stream<Item = M> + Send + 'static,
    {
        Self {
            entries: vec![SubscriptionEntry {
                key,
                stream: Box::pin(stream),
            }],
        }
    }

    /// Add a stream that emits `message` once at the end.
    ///
    /// A bare stream end is invisible to the actor: its key stays active until
    /// the application stops returning it. Use this constructor when the
    /// application must observe completion and then return a new key (or omit
    /// the old one). The stream yielded by this call emits `message` once and
    /// then ends; the key remains active until reconciliation.
    pub fn stream_with_end<S>(key: u64, stream: S, message: M) -> Self
    where
        M: Send + 'static,
        S: Stream<Item = M> + Send + 'static,
    {
        Self::stream(
            key,
            stream.chain(futures::stream::once(async move { message })),
        )
    }

    /// Add a periodic timer stream under `key`.
    ///
    /// The first tick arrives after `period`. Change the key when the period
    /// changes. Periods are clamped to
    /// one millisecond and missed ticks are skipped rather than burst.
    pub fn interval(key: u64, period: Duration, message: M) -> Self
    where
        M: Clone + Send + 'static,
    {
        let period = period.max(Duration::from_millis(1));
        let mut ticker = interval_at(Instant::now() + period, period);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let stream = futures::stream::unfold(ticker, move |mut ticker| {
            let message = message.clone();
            async move {
                ticker.tick().await;
                Some((message, ticker))
            }
        });
        Self::stream(key, stream)
    }

    /// Combine subscriptions. Entries are spawned concurrently.
    pub fn batch(subscriptions: impl IntoIterator<Item = Self>) -> Self {
        Self {
            entries: subscriptions
                .into_iter()
                .flat_map(|subscription| subscription.entries)
                .collect(),
        }
    }

    /// Whether this subscription contains no streams.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub(crate) fn into_entries(self) -> Vec<SubscriptionEntry<M>> {
        self.entries
    }
}

impl<M> Default for Subscription<M> {
    fn default() -> Self {
        Self::none()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use futures::StreamExt;
    use tokio::time::timeout;

    use super::*;

    #[tokio::test]
    async fn stream_with_end_emits_one_terminal_message() {
        let mut subscription = Subscription::stream_with_end(1, futures::stream::iter([1, 2]), 3);
        let entry = subscription.entries.remove(0);
        let mut stream = entry.stream;
        assert_eq!(stream.next().await, Some(1));
        assert_eq!(stream.next().await, Some(2));
        assert_eq!(stream.next().await, Some(3));
        assert_eq!(stream.next().await, None);
    }

    #[tokio::test]
    async fn zero_interval_is_clamped_instead_of_panicking() {
        let mut subscription = Subscription::interval(1, Duration::ZERO, 7);
        let entry = subscription.entries.remove(0);
        let mut stream = entry.stream;
        assert_eq!(
            timeout(Duration::from_secs(1), stream.next())
                .await
                .expect("clamped tick arrives"),
            Some(7)
        );
    }

    #[tokio::test]
    async fn interval_emits_after_the_requested_period() {
        let mut subscription = Subscription::interval(1, Duration::from_millis(10), 7);
        let entry = subscription.entries.remove(0);
        let mut stream = entry.stream;
        assert_eq!(
            timeout(Duration::from_secs(1), stream.next())
                .await
                .expect("tick arrives"),
            Some(7)
        );
    }
}
