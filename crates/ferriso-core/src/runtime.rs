//! Actor runtime: one task owns the application and talks to frontends only
//! through channels.
//!
//! ```text
//! frontend ──CoreEvent::Message──▶ actor ──Arc<View> snapshot──▶ frontend
//!                                 │
//!                                 └── Tokio commands / timer
//! ```
//!
//! The event type is generic over the application message; no ratatui,
//! crossterm, or iced type can cross this boundary.

use std::{
    hash::{Hash, Hasher},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
    time::{Instant, Interval, interval_at},
};

use crate::{App, Command};

static NEXT_CHANNEL_ID: AtomicU64 = AtomicU64::new(0);

/// Input accepted by the core actor.
///
/// Frontends construct [`CoreEvent::Message`] from semantic input; `Shutdown`
/// is lifecycle control. Nothing in this enum describes a terminal or a window.
#[derive(Debug)]
pub enum CoreEvent<M> {
    Message(M),
    Shutdown,
}

/// A cheap, cloneable sender into the actor.
#[derive(Debug)]
pub struct CoreHandle<M> {
    tx: mpsc::UnboundedSender<CoreEvent<M>>,
}

impl<M> Clone for CoreHandle<M> {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
        }
    }
}

impl<M> CoreHandle<M> {
    /// Send one semantic message. Returns `false` once the actor has stopped.
    pub fn send(&self, message: M) -> bool {
        self.tx.send(CoreEvent::Message(message)).is_ok()
    }

    /// Ask the actor to stop after the messages already queued.
    pub fn shutdown(&self) {
        let _ = self.tx.send(CoreEvent::Shutdown);
    }

    /// Whether the actor has stopped (or its task has been aborted).
    pub fn is_closed(&self) -> bool {
        self.tx.is_closed()
    }
}

/// Read side of the snapshot channel.
///
/// `watch` keeps only the newest view, so a slow frontend naturally coalesces
/// updates instead of building a backlog. Each snapshot is an [`Arc`], making
/// the clone into a frontend cheap.
pub struct Snapshots<V> {
    rx: watch::Receiver<Arc<V>>,
    id: u64,
}

impl<V> Clone for Snapshots<V> {
    fn clone(&self) -> Self {
        Self {
            rx: self.rx.clone(),
            id: self.id,
        }
    }
}

impl<V> Hash for Snapshots<V> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl<V> Snapshots<V> {
    /// Latest snapshot, cloned out of the channel.
    pub fn current(&self) -> Arc<V> {
        self.rx.borrow().clone()
    }

    /// Wait for the next snapshot. Returns `Err` after the actor exits.
    pub async fn changed(&mut self) -> Result<Arc<V>, watch::error::RecvError> {
        self.rx.changed().await?;
        Ok(self.rx.borrow_and_update().clone())
    }

    /// Whether the actor has stopped publishing.
    pub fn closed(&self) -> bool {
        self.rx.has_changed().is_err()
    }
}

/// Handle to the actor task.
pub struct CoreTask {
    handle: JoinHandle<()>,
}

impl CoreTask {
    /// Wait for the actor to finish.
    pub async fn join(self) {
        let _ = self.handle.await;
    }

    /// Stop the actor immediately, without running [`App::shutdown`].
    pub fn abort(&self) {
        self.handle.abort();
    }
}

/// Start an application actor.
///
/// Must be called inside a Tokio runtime. The returned handle is the only way
/// to mutate the application; the snapshots are the only way to observe it.
pub fn spawn<A: App>(app: A) -> (CoreHandle<A::Message>, Snapshots<A::View>, CoreTask) {
    let mut app = app;
    // `init` mutates state before anyone can observe it, so the very first
    // snapshot already includes initialization. Its commands run on the actor.
    let init = app.init();
    let (views_tx, views_rx) = watch::channel(Arc::new(app.view()));
    let (control_tx, mut control_rx) = mpsc::unbounded_channel();
    let (command_tx, mut command_rx) = mpsc::unbounded_channel::<A::Message>();
    let handle = CoreHandle { tx: control_tx };

    let task = tokio::spawn(async move {
        let mut interval_duration = app.tick_interval();
        let mut ticker = interval_duration.map(|duration| {
            interval_at(
                Instant::now() + duration.max(Duration::from_millis(1)),
                duration,
            )
        });

        dispatch(init, &command_tx);
        if app.should_quit() {
            app.shutdown();
            return;
        }

        loop {
            tokio::select! {
                biased;
                event = control_rx.recv() => {
                    match event {
                        Some(CoreEvent::Message(message)) => {
                            dispatch(app.update(message), &command_tx);
                        }
                        Some(CoreEvent::Shutdown) | None => break,
                    }
                    refresh_ticker(&app, &mut interval_duration, &mut ticker);
                    views_tx.send_replace(Arc::new(app.view()));
                    if app.should_quit() {
                        break;
                    }
                }
                Some(message) = command_rx.recv() => {
                    dispatch(app.update(message), &command_tx);
                    refresh_ticker(&app, &mut interval_duration, &mut ticker);
                    views_tx.send_replace(Arc::new(app.view()));
                    if app.should_quit() {
                        break;
                    }
                }
                _ = next_tick(&mut ticker) => {
                    dispatch(app.tick(), &command_tx);
                    views_tx.send_replace(Arc::new(app.view()));
                    if app.should_quit() {
                        break;
                    }
                }
            }
        }

        // Drop the command sender so in-flight tasks stop queueing work.
        drop(command_tx);
        app.shutdown();
    });

    let snapshots = Snapshots {
        rx: views_rx,
        id: NEXT_CHANNEL_ID.fetch_add(1, Ordering::Relaxed),
    };
    (handle, snapshots, CoreTask { handle: task })
}

fn dispatch<M: Send + 'static>(command: Command<M>, tx: &mpsc::UnboundedSender<M>) {
    for future in command.into_futures() {
        let tx = tx.clone();
        tokio::spawn(async move {
            let message = future.await;
            let _ = tx.send(message);
        });
    }
}

fn refresh_ticker<A: App>(app: &A, current: &mut Option<Duration>, ticker: &mut Option<Interval>) {
    let wanted = app.tick_interval();
    if wanted != *current {
        *current = wanted;
        *ticker = wanted.map(|duration| {
            interval_at(
                Instant::now() + duration.max(Duration::from_millis(1)),
                duration,
            )
        });
    }
}

async fn next_tick(ticker: &mut Option<Interval>) {
    match ticker {
        Some(ticker) => {
            ticker.tick().await;
        }
        None => std::future::pending::<()>().await,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::time::timeout;

    use super::*;
    use crate::Command;

    #[derive(Default)]
    struct Counter {
        count: u32,
        quit: bool,
    }

    enum Message {
        Increment,
        Quit,
    }

    impl App for Counter {
        type Message = Message;
        type View = u32;

        fn init(&mut self) -> Command<Message> {
            Command::perform(async { Message::Increment }, |message| message)
        }

        fn update(&mut self, message: Message) -> Command<Message> {
            match message {
                Message::Increment => {
                    self.count += 1;
                    if self.count < 2 {
                        Command::perform(async { Message::Increment }, |message| message)
                    } else {
                        Command::none()
                    }
                }
                Message::Quit => {
                    self.quit = true;
                    Command::none()
                }
            }
        }

        fn view(&self) -> u32 {
            self.count
        }

        fn should_quit(&self) -> bool {
            self.quit
        }
    }

    #[tokio::test]
    async fn first_snapshot_includes_init_state() {
        struct Initialized {
            ready: bool,
        }

        impl App for Initialized {
            type Message = ();
            type View = bool;

            fn init(&mut self) -> Command<()> {
                self.ready = true;
                Command::none()
            }

            fn update(&mut self, (): ()) -> Command<()> {
                Command::none()
            }

            fn view(&self) -> bool {
                self.ready
            }
        }

        let (handle, snapshots, task) = spawn(Initialized { ready: false });
        assert!(*snapshots.current());
        drop(handle);
        timeout(Duration::from_secs(1), task.join())
            .await
            .expect("actor exits");
    }

    #[tokio::test]
    async fn commands_round_trip_and_snapshots_follow() {
        let (handle, mut snapshots, task) = spawn(Counter::default());
        let view = timeout(Duration::from_secs(1), async {
            loop {
                let view = snapshots.changed().await.expect("actor alive");
                if *view == 2 {
                    break view;
                }
            }
        })
        .await
        .expect("two command results");
        assert_eq!(*view, 2);

        assert!(handle.send(Message::Quit));
        timeout(Duration::from_secs(1), task.join())
            .await
            .expect("actor exits");
    }

    #[tokio::test]
    async fn tick_runs_on_the_actor_timer() {
        struct Ticker {
            ticks: u32,
        }

        impl App for Ticker {
            type Message = ();
            type View = u32;

            fn update(&mut self, (): ()) -> Command<()> {
                Command::none()
            }

            fn view(&self) -> u32 {
                self.ticks
            }

            fn tick_interval(&self) -> Option<Duration> {
                Some(Duration::from_millis(10))
            }

            fn tick(&mut self) -> Command<()> {
                self.ticks += 1;
                Command::none()
            }
        }

        let (_handle, mut snapshots, task) = spawn(Ticker { ticks: 0 });
        timeout(Duration::from_secs(1), async {
            loop {
                if *snapshots.changed().await.expect("actor alive") >= 3 {
                    break;
                }
            }
        })
        .await
        .expect("ticks arrive");
        task.abort();
    }

    #[tokio::test]
    async fn dropping_every_handle_stops_the_actor() {
        let (handle, snapshots, task) = spawn(Counter::default());
        drop(handle);
        timeout(Duration::from_secs(1), task.join())
            .await
            .expect("actor exits when no frontend remains");
        assert!(snapshots.closed());
    }
}
