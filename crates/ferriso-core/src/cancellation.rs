//! Cooperative cancellation for application-owned work.
//!
//! A [`CancellationToken`] is a small, cloneable signal. Keep one in app state,
//! clone it into a command future, and cancel it from `update` when the work is
//! no longer relevant. The future decides how to stop; Ferriso does not abort
//! arbitrary user futures behind the application's back.
//!
//! ```no_run
//! use ferriso_core::{CancellationToken, Command};
//!
//! # async fn work() {}
//! # struct App { token: CancellationToken }
//! # impl App {
//! # fn cancel(&self) { self.token.cancel(); }
//! # fn start(&self) -> Command<()> {
//! let token = self.token.clone();
//! Command::perform(
//!     async move {
//!         tokio::select! {
//!             _ = work() => {}
//!             _ = token.cancelled() => {}
//!         }
//!     },
//!     |_| (),
//! )
//! # }
//! # }
//! ```

use tokio::sync::watch;

/// A cloneable cancellation signal.
#[derive(Clone, Debug)]
pub struct CancellationToken {
    cancelled: watch::Sender<bool>,
}

impl CancellationToken {
    /// Create an uncancelled token.
    pub fn new() -> Self {
        let (cancelled, _) = watch::channel(false);
        Self { cancelled }
    }

    /// Cancel the token. Idempotent; all current and future waiters observe it.
    pub fn cancel(&self) {
        self.cancelled.send_replace(true);
    }

    /// Whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        *self.cancelled.borrow()
    }

    /// Wait until cancellation is requested.
    ///
    /// Returns immediately when the token is already cancelled. If the token's
    /// sender is dropped without cancelling, this also returns.
    pub async fn cancelled(&self) {
        let mut receiver = self.cancelled.subscribe();
        loop {
            if *receiver.borrow_and_update() {
                return;
            }
            if receiver.changed().await.is_err() {
                return;
            }
        }
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::time::timeout;

    use super::*;

    #[tokio::test]
    async fn cancel_wakes_waiters() {
        let token = CancellationToken::new();
        let waiter = token.clone();
        let wait = tokio::spawn(async move { waiter.cancelled().await });

        token.cancel();
        timeout(Duration::from_secs(1), wait)
            .await
            .expect("waiter wakes")
            .expect("waiter task completes");
        assert!(token.is_cancelled());
    }

    #[tokio::test]
    async fn cancel_before_wait_returns_immediately_and_is_idempotent() {
        let token = CancellationToken::new();
        token.cancel();
        token.cancel();

        timeout(Duration::from_secs(1), token.cancelled())
            .await
            .expect("already-cancelled token completes");
    }

    #[tokio::test]
    async fn clones_observe_the_same_cancellation() {
        let token = CancellationToken::new();
        let clone = token.clone();

        clone.cancel();
        assert!(token.is_cancelled());
        timeout(Duration::from_secs(1), token.cancelled())
            .await
            .expect("original sees clone cancellation");
    }
}
