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
//!
//! ## Boundaries
//!
//! Everything the framework calls is user code: `init`, `update`, `tick`,
//! `view`, `should_quit`, `shutdown`, `debug`, and command futures. The actor
//! wraps those calls; a panic is converted into a [`FrameworkError`] with the
//! phase and task id, recorded in [`Diagnostics`], and never silently swallows
//! the task. Startup panics are logged and re-raised.

use std::{
    any::Any,
    hash::{Hash, Hasher},
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use ferriso_diagnostics::{Diagnostics, EventKind, FrameworkError, Level, TaskId};
use futures::FutureExt;
use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
    time::{Instant, Interval, interval_at},
};

use crate::{App, Command};

static NEXT_CHANNEL_ID: AtomicU64 = AtomicU64::new(0);

const TARGET: &str = "ferriso-core::actor";

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

/// Start an application actor with a quiet diagnostics handle.
///
/// Must be called inside a Tokio runtime. The returned handle is the only way
/// to mutate the application; the snapshots are the only way to observe it.
/// Panics are still contained, but nothing is logged. Use
/// [`spawn_with_diagnostics`] when the frontend or application should see
/// framework errors and debug events.
pub fn spawn<A: App>(app: A) -> (CoreHandle<A::Message>, Snapshots<A::View>, CoreTask) {
    spawn_inner(app, Diagnostics::new())
}

/// Start an application actor and return its enabled diagnostics tap.
///
/// Equivalent to [`spawn_with_diagnostics`] with `Diagnostics::new().with_debug(true)`.
pub fn spawn_with_debug<A: App>(
    app: A,
) -> (
    CoreHandle<A::Message>,
    Snapshots<A::View>,
    CoreTask,
    Diagnostics,
) {
    let diagnostics = Diagnostics::new().with_debug(true);
    let (handle, snapshots, task) = spawn_inner(app, diagnostics.clone());
    (handle, snapshots, task, diagnostics)
}

/// Start an application actor with caller-owned diagnostics.
///
/// The same handle can be shared with loggers, diagnostics exporters, and
/// frontend overlays. Debug mode is opt-in on the handle itself.
pub fn spawn_with_diagnostics<A: App>(
    app: A,
    diagnostics: Diagnostics,
) -> (CoreHandle<A::Message>, Snapshots<A::View>, CoreTask) {
    spawn_inner(app, diagnostics)
}

fn spawn_inner<A: App>(
    app: A,
    diagnostics: Diagnostics,
) -> (CoreHandle<A::Message>, Snapshots<A::View>, CoreTask) {
    let mut app = app;
    let task_id = diagnostics.next_task();
    diagnostics.event(
        EventKind::Lifecycle,
        TARGET,
        "init",
        task_id,
        std::any::type_name::<A>(),
    );
    diagnostics.log(Level::Info, TARGET, "init", task_id, "actor starting");

    // `init` mutates state before anyone can observe it, so the very first
    // snapshot already includes initialization. Its commands run on the actor.
    let init = startup(&diagnostics, task_id, "init", || app.init());
    let initial_view = startup(&diagnostics, task_id, "view", || app.view());

    let (views_tx, views_rx) = watch::channel(Arc::new(initial_view));
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
        let mut snapshot_sequence = 1_u64;

        diagnostics.event(
            EventKind::Snapshot,
            TARGET,
            "init",
            task_id,
            format!("initial #{snapshot_sequence}"),
        );
        dispatch(init, &command_tx, &diagnostics, task_id);

        let quit_after_start =
            guard(&diagnostics, task_id, "should_quit", || app.should_quit()).unwrap_or(true);

        if !quit_after_start {
            'actor: loop {
                tokio::select! {
                    biased;
                    event = control_rx.recv() => {
                        match event {
                            Some(CoreEvent::Message(message)) => {
                                if diagnostics.debug_enabled() {
                                    let description =
                                        describe(&app, &message, &diagnostics, task_id);
                                    diagnostics.event(
                                        EventKind::Message,
                                        TARGET,
                                        "update",
                                        task_id,
                                        description,
                                    );
                                }

                                let Some(command) =
                                    guard(&diagnostics, task_id, "update", || app.update(message))
                                else {
                                    break 'actor;
                                };
                                dispatch(command, &command_tx, &diagnostics, task_id);
                            }
                            Some(CoreEvent::Shutdown) | None => break 'actor,
                        }

                        if !publish(
                            &app,
                            &diagnostics,
                            task_id,
                            &mut interval_duration,
                            &mut ticker,
                            &views_tx,
                            &mut snapshot_sequence,
                            "update",
                        ) {
                            break 'actor;
                        }
                    }
                    Some(message) = command_rx.recv() => {
                        if diagnostics.debug_enabled() {
                            let description =
                                describe(&app, &message, &diagnostics, task_id);
                            diagnostics.event(
                                EventKind::CommandResult,
                                TARGET,
                                "command",
                                task_id,
                                description,
                            );
                        }

                        let Some(command) =
                            guard(&diagnostics, task_id, "update", || app.update(message))
                        else {
                            break 'actor;
                        };
                        dispatch(command, &command_tx, &diagnostics, task_id);

                        if !publish(
                            &app,
                            &diagnostics,
                            task_id,
                            &mut interval_duration,
                            &mut ticker,
                            &views_tx,
                            &mut snapshot_sequence,
                            "command",
                        ) {
                            break 'actor;
                        }
                    }
                    _ = next_tick(&mut ticker) => {
                        diagnostics.event(EventKind::Tick, TARGET, "tick", task_id, "timer");

                        let Some(command) =
                            guard(&diagnostics, task_id, "tick", || app.tick())
                        else {
                            break 'actor;
                        };
                        dispatch(command, &command_tx, &diagnostics, task_id);

                        if !publish(
                            &app,
                            &diagnostics,
                            task_id,
                            &mut interval_duration,
                            &mut ticker,
                            &views_tx,
                            &mut snapshot_sequence,
                            "tick",
                        ) {
                            break 'actor;
                        }
                    }
                }
            }
        }

        diagnostics.event(
            EventKind::Lifecycle,
            TARGET,
            "shutdown",
            task_id,
            "actor stopped",
        );
        diagnostics.log(Level::Info, TARGET, "shutdown", task_id, "actor stopped");

        // Drop the command sender so in-flight tasks stop queueing work.
        drop(command_tx);
        let _ = guard(&diagnostics, task_id, "shutdown", || app.shutdown());
    });

    let snapshots = Snapshots {
        rx: views_rx,
        id: NEXT_CHANNEL_ID.fetch_add(1, Ordering::Relaxed),
    };
    (handle, snapshots, CoreTask { handle: task })
}

/// Guard one user-code call. On panic, record a phase-tagged framework error
/// and return `None`; callers decide whether to stop the actor.
fn guard<T>(
    diagnostics: &Diagnostics,
    task: TaskId,
    phase: &'static str,
    user_code: impl FnOnce() -> T,
) -> Option<T> {
    match catch_unwind(AssertUnwindSafe(user_code)) {
        Ok(value) => Some(value),
        Err(payload) => {
            diagnostics.error(
                phase,
                task,
                &FrameworkError::ActorPanic {
                    phase,
                    message: panic_message(payload.as_ref()),
                    task: task.get(),
                },
            );
            None
        }
    }
}

/// Like [`guard`], but a panic at startup is logged and then re-raised so the
/// caller cannot continue with an uninitialized actor.
fn startup<T>(
    diagnostics: &Diagnostics,
    task: TaskId,
    phase: &'static str,
    user_code: impl FnOnce() -> T,
) -> T {
    match catch_unwind(AssertUnwindSafe(user_code)) {
        Ok(value) => value,
        Err(payload) => {
            diagnostics.error(
                phase,
                task,
                &FrameworkError::ActorPanic {
                    phase,
                    message: panic_message(payload.as_ref()),
                    task: task.get(),
                },
            );
            resume_unwind(payload)
        }
    }
}

/// Refresh the timer, publish a snapshot, and report whether the actor should
/// continue. A panic in any hook stops the actor instead of publishing
/// possibly inconsistent state.
#[allow(clippy::too_many_arguments)]
fn publish<A: App>(
    app: &A,
    diagnostics: &Diagnostics,
    task: TaskId,
    interval_duration: &mut Option<Duration>,
    ticker: &mut Option<Interval>,
    views_tx: &watch::Sender<Arc<A::View>>,
    snapshot_sequence: &mut u64,
    phase: &'static str,
) -> bool {
    let Some(wanted) = guard(diagnostics, task, "tick_interval", || app.tick_interval()) else {
        return false;
    };

    if wanted != *interval_duration {
        *interval_duration = wanted;
        *ticker = wanted.map(|duration| {
            interval_at(
                Instant::now() + duration.max(Duration::from_millis(1)),
                duration,
            )
        });
    }

    let Some(view) = guard(diagnostics, task, "view", || app.view()) else {
        return false;
    };

    *snapshot_sequence += 1;
    diagnostics.event(
        EventKind::Snapshot,
        TARGET,
        phase,
        task,
        format!("{phase} #{snapshot_sequence}"),
    );
    views_tx.send_replace(Arc::new(view));

    match guard(diagnostics, task, "should_quit", || app.should_quit()) {
        Some(quit) => !quit,
        None => false,
    }
}

fn dispatch<M: Send + 'static>(
    command: Command<M>,
    tx: &mpsc::UnboundedSender<M>,
    diagnostics: &Diagnostics,
    task: TaskId,
) {
    for future in command.into_futures() {
        diagnostics.event(EventKind::Command, TARGET, "command", task, "spawned");
        let tx = tx.clone();
        let diagnostics = diagnostics.clone();
        let joined = AssertUnwindSafe(future).catch_unwind();

        tokio::spawn(async move {
            match joined.await {
                Ok(message) => {
                    let _ = tx.send(message);
                }
                Err(payload) => {
                    diagnostics.error(
                        "command",
                        task,
                        &FrameworkError::CommandPanic {
                            phase: "command",
                            message: panic_message(payload.as_ref()),
                            task: task.get(),
                        },
                    );
                }
            }
        });
    }
}

fn describe<A: App>(
    app: &A,
    message: &A::Message,
    diagnostics: &Diagnostics,
    task: TaskId,
) -> String {
    match catch_unwind(AssertUnwindSafe(|| app.debug(message))) {
        Ok(Some(description)) => description,
        Ok(None) => std::any::type_name::<A::Message>().to_owned(),
        Err(payload) => {
            diagnostics.log(
                Level::Warn,
                TARGET,
                "debug",
                task,
                &format!("App::debug panicked: {}", panic_message(payload.as_ref())),
            );
            std::any::type_name::<A::Message>().to_owned()
        }
    }
}

fn panic_message(payload: &(dyn Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_owned()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "non-string panic payload".into()
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

        fn debug(&self, message: &Message) -> Option<String> {
            Some(match message {
                Message::Increment => "increment".into(),
                Message::Quit => "quit".into(),
            })
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

    #[tokio::test]
    async fn diagnostics_record_messages_commands_and_snapshots() {
        let diagnostics = Diagnostics::new().with_debug(true);
        let (handle, mut snapshots, task) =
            spawn_with_diagnostics(Counter::default(), diagnostics.clone());

        timeout(Duration::from_secs(1), async {
            loop {
                if *snapshots.changed().await.expect("actor alive") == 2 {
                    break;
                }
            }
        })
        .await
        .expect("command results arrive");

        handle.send(Message::Quit);
        timeout(Duration::from_secs(1), task.join())
            .await
            .expect("actor exits");

        let events = diagnostics.events();
        let kinds: Vec<_> = events.iter().map(|event| event.kind).collect();
        assert!(kinds.contains(&EventKind::Message));
        assert!(kinds.contains(&EventKind::Command));
        assert!(kinds.contains(&EventKind::CommandResult));
        assert!(kinds.contains(&EventKind::Snapshot));
        assert!(kinds.contains(&EventKind::Lifecycle));
        assert!(events.iter().any(|event| event.detail == "increment"));
        assert!(events.iter().any(|event| event.detail == "quit"));
        assert!(diagnostics.recent_errors().is_empty());
        assert!(events.iter().all(|event| event.task.get() == 1));
    }

    #[tokio::test]
    async fn update_panic_is_wrapped_and_stops_the_actor() {
        struct Panicker;

        impl App for Panicker {
            type Message = ();
            type View = ();

            fn update(&mut self, (): ()) -> Command<()> {
                panic!("update exploded");
            }

            fn view(&self) {}
        }

        let diagnostics = Diagnostics::new().with_debug(true);
        let (handle, _snapshots, task) = spawn_with_diagnostics(Panicker, diagnostics.clone());
        assert!(handle.send(()));

        timeout(Duration::from_secs(1), task.join())
            .await
            .expect("actor stops at the boundary");

        let errors = diagnostics.recent_errors();
        let error = errors.last().expect("panic recorded");
        assert_eq!(error.code.as_deref(), Some("ferriso::actor::panic"));
        assert_eq!(error.phase, "update");
        assert!(error.message.contains("update exploded"));
        assert!(error.help.is_some());
    }

    #[tokio::test]
    async fn command_panic_is_wrapped_and_the_actor_survives() {
        struct BadCommand;

        impl App for BadCommand {
            type Message = ();
            type View = ();

            fn init(&mut self) -> Command<()> {
                Command::perform(
                    async {
                        panic!("command exploded");
                    },
                    |(): ()| (),
                )
            }

            fn update(&mut self, (): ()) -> Command<()> {
                Command::none()
            }

            fn view(&self) {}
        }

        let diagnostics = Diagnostics::new().with_debug(true);
        let (handle, _snapshots, task) = spawn_with_diagnostics(BadCommand, diagnostics.clone());

        timeout(Duration::from_secs(1), async {
            loop {
                if !diagnostics.recent_errors().is_empty() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("command panic recorded");

        let errors = diagnostics.recent_errors();
        let error = errors.last().expect("panic recorded");
        assert_eq!(error.code.as_deref(), Some("ferriso::command::panic"));
        assert!(error.message.contains("command exploded"));

        assert!(!handle.is_closed(), "actor survives a command panic");
        handle.shutdown();
        timeout(Duration::from_secs(1), task.join())
            .await
            .expect("actor exits after shutdown");
    }
}
