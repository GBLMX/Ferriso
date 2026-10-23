//! Actor runtime: one task owns the application and talks to frontends only
//! through channels.
//!
//! ```text
//! frontend ──CoreEvent::Message──▶ actor ──Arc<View> snapshot──▶ frontend
//!                                 │
//!                                 └── commands / subscriptions / deprecated timer
//! ```
//!
//! The event type is generic over the application message; no ratatui,
//! crossterm, or iced type can cross this boundary.
//!
//! ## Boundaries
//!
//! Everything the framework calls is user code: `init`, `update`, the
//! deprecated `tick`, `view`, `subscriptions`, `should_quit`, `shutdown`,
//! `debug`, command futures, and subscription streams. The actor wraps those
//! calls; a panic is converted into a [`FrameworkError`] with the phase and task
//! id, recorded in [`Diagnostics`], and stops the actor instead of continuing
//! on inconsistent state. Startup panics are logged and re-raised; a
//! [`App::debug`] panic is diagnostic-only, and shutdown panics are recorded
//! after the actor has already stopped. Shutdown awaits [`App::shutdown_async`]
//! before calling [`App::shutdown`].

use std::{
    any::Any,
    collections::{HashMap, HashSet},
    hash::{Hash, Hasher},
    num::NonZeroUsize,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use ferriso_diagnostics::{Diagnostics, EventKind, FrameworkError, Level, TaskId};
use futures::{FutureExt, StreamExt};
use tokio::{
    sync::{mpsc, watch},
    task::{JoinError, JoinHandle, JoinSet},
    time::{Instant, Interval, MissedTickBehavior, interval_at},
};

use crate::{App, Command};

static NEXT_CHANNEL_ID: AtomicU64 = AtomicU64::new(0);

const TARGET: &str = "ferriso-core::actor";
/// Command and subscription results share one bounded inbox. When it is full,
/// producers wait instead of growing memory without limit.
const INBOX_CAPACITY: usize = 256;

/// Runtime tuning for an actor.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct RuntimeOptions {
    /// Capacity of the command/subscription result inbox.
    pub inbox_capacity: NonZeroUsize,
}

impl RuntimeOptions {
    /// Build options with an explicit inbox capacity.
    pub fn new(inbox_capacity: NonZeroUsize) -> Self {
        Self { inbox_capacity }
    }
}

impl Default for RuntimeOptions {
    fn default() -> Self {
        Self {
            inbox_capacity: NonZeroUsize::new(INBOX_CAPACITY).expect("non-zero default capacity"),
        }
    }
}

enum InboxMessage<M> {
    Command { message: M, task: TaskId },
    Subscription { message: M, task: TaskId },
    Fatal { phase: &'static str, task: TaskId },
}

/// Abort the subscription task even when the actor task itself is aborted.
/// Dropping a bare JoinHandle detaches the task instead.
struct SubscriptionTask {
    handle: JoinHandle<()>,
}

impl SubscriptionTask {
    fn abort(&self) {
        self.handle.abort();
    }
}

impl Drop for SubscriptionTask {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

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
    /// Send one semantic message without waiting for a frame or actor work.
    ///
    /// The control channel is intentionally unbounded; frontends must coalesce
    /// high-frequency input such as mouse movement, resize, and scroll events
    /// before calling this method. Returns `false` once the actor has stopped.
    pub fn send(&self, message: M) -> bool {
        self.tx.send(CoreEvent::Message(message)).is_ok()
    }

    /// Ask the actor to stop after the messages already queued.
    ///
    /// The actor aborts in-flight work, then awaits [`App::shutdown_async`],
    /// then calls [`App::shutdown`].
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
    rx: watch::Receiver<(Arc<V>, u64)>,
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
        self.current_with_revision().0
    }

    /// Latest snapshot and its revision.
    ///
    /// Revision `0` is the boot snapshot. Each publish increments the revision,
    /// even when the view value is unchanged. A slow reader can skip revisions
    /// because `watch` coalesces; the returned view and revision are taken from
    /// the same published value.
    pub fn current_with_revision(&self) -> (Arc<V>, u64) {
        let published = self.rx.borrow();
        (published.0.clone(), published.1)
    }

    /// Revision of the latest snapshot.
    pub fn revision(&self) -> u64 {
        self.rx.borrow().1
    }

    /// Wait for the next snapshot. Returns `Err` after the actor exits.
    pub async fn changed(&mut self) -> Result<Arc<V>, watch::error::RecvError> {
        self.changed_with_revision().await.map(|(view, _)| view)
    }

    /// Wait for the next snapshot and its revision.
    pub async fn changed_with_revision(
        &mut self,
    ) -> Result<(Arc<V>, u64), watch::error::RecvError> {
        self.rx.changed().await?;
        let published = self.rx.borrow_and_update();
        Ok((published.0.clone(), published.1))
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
    /// Wait for the actor to finish, discarding a join failure.
    ///
    /// Prefer [`CoreTask::join_checked`] when the caller needs to distinguish a
    /// clean shutdown from a cancelled or panicked actor task.
    pub async fn join(self) {
        let _ = self.handle.await;
    }

    /// Wait for the actor and report cancellation or a panic in the actor task.
    pub async fn join_checked(self) -> Result<(), JoinError> {
        self.handle.await
    }

    /// Stop the actor immediately, without running [`App::shutdown`].
    /// In-flight commands and subscription tasks are aborted with it.
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
    spawn_with_options(app, RuntimeOptions::default())
}

/// Start an application actor with explicit runtime options and quiet
/// diagnostics.
pub fn spawn_with_options<A: App>(
    app: A,
    options: RuntimeOptions,
) -> (CoreHandle<A::Message>, Snapshots<A::View>, CoreTask) {
    spawn_inner(app, Diagnostics::new(), options)
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
    let (handle, snapshots, task) =
        spawn_inner(app, diagnostics.clone(), RuntimeOptions::default());
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
    spawn_inner(app, diagnostics, RuntimeOptions::default())
}

/// Start an application actor with caller-owned diagnostics and explicit
/// runtime options.
pub fn spawn_with_diagnostics_and_options<A: App>(
    app: A,
    diagnostics: Diagnostics,
    options: RuntimeOptions,
) -> (CoreHandle<A::Message>, Snapshots<A::View>, CoreTask) {
    spawn_inner(app, diagnostics, options)
}

fn spawn_inner<A: App>(
    app: A,
    diagnostics: Diagnostics,
    options: RuntimeOptions,
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

    let (views_tx, views_rx) = watch::channel((Arc::new(initial_view), 0));
    let (control_tx, mut control_rx) = mpsc::unbounded_channel();
    let (command_tx, mut command_rx) =
        mpsc::channel::<InboxMessage<A::Message>>(options.inbox_capacity.get());
    let handle = CoreHandle { tx: control_tx };

    let task = tokio::spawn(async move {
        let mut interval_duration = startup(&diagnostics, task_id, "tick_interval", || {
            app_tick_interval(&app)
        });
        let mut ticker = interval_duration.map(make_ticker);
        let mut snapshot_sequence = 1_u64;
        let mut active_subscriptions: HashMap<u64, SubscriptionTask> = HashMap::new();
        let mut commands = JoinSet::new();

        if diagnostics.debug_enabled() {
            diagnostics.event(
                EventKind::Snapshot,
                TARGET,
                "init",
                task_id,
                format!("initial #{snapshot_sequence}"),
            );
        }
        dispatch(init, &mut commands, &command_tx, &diagnostics);

        let quit_after_start =
            guard(&diagnostics, task_id, "should_quit", || app.should_quit()).unwrap_or(true);

        if !quit_after_start
            && reconcile_subscriptions(
                &app,
                &mut active_subscriptions,
                &command_tx,
                &diagnostics,
                task_id,
            )
        {
            'actor: loop {
                reap_commands(&mut commands, &diagnostics, task_id);

                tokio::select! {
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
                                dispatch(command, &mut commands, &command_tx, &diagnostics);
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
                            &mut active_subscriptions,
                            &command_tx,
                            "update",
                        ) {
                            break 'actor;
                        }
                    }
                    Some(message) = command_rx.recv() => {
                        let (kind, phase, message, source) = match message {
                            InboxMessage::Command { message, task } => {
                                (EventKind::CommandResult, "command", message, task)
                            }
                            InboxMessage::Subscription { message, task } => {
                                (EventKind::Subscription, "subscription", message, task)
                            }
                            InboxMessage::Fatal { phase, task } => {
                                diagnostics.log(
                                    Level::Error,
                                    TARGET,
                                    phase,
                                    task_id,
                                    &format!(
                                        "task {} panicked; stopping the actor",
                                        task.get()
                                    ),
                                );
                                break 'actor;
                            }
                        };

                        if diagnostics.debug_enabled() {
                            let description =
                                describe(&app, &message, &diagnostics, source);
                            diagnostics.event(kind, TARGET, phase, source, description);
                        }

                        let Some(command) =
                            guard(&diagnostics, task_id, "update", || app.update(message))
                        else {
                            break 'actor;
                        };
                        dispatch(command, &mut commands, &command_tx, &diagnostics);

                        if !publish(
                            &app,
                            &diagnostics,
                            task_id,
                            &mut interval_duration,
                            &mut ticker,
                            &views_tx,
                            &mut snapshot_sequence,
                            &mut active_subscriptions,
                            &command_tx,
                            phase,
                        ) {
                            break 'actor;
                        }
                    }
                    _ = next_tick(&mut ticker) => {
                        diagnostics.event(EventKind::Tick, TARGET, "tick", task_id, "timer");

                        let Some(command) = guard(&diagnostics, task_id, "tick", || {
                            app_tick(&mut app)
                        }) else {
                            break 'actor;
                        };
                        dispatch(command, &mut commands, &command_tx, &diagnostics);

                        if !publish(
                            &app,
                            &diagnostics,
                            task_id,
                            &mut interval_duration,
                            &mut ticker,
                            &views_tx,
                            &mut snapshot_sequence,
                            &mut active_subscriptions,
                            &command_tx,
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

        commands.abort_all();
        for (_, handle) in active_subscriptions {
            handle.abort();
        }

        // Drop the command sender so in-flight tasks stop queueing work.
        drop(command_tx);
        {
            let shutdown_future = guard(&diagnostics, task_id, "shutdown_async", || {
                app.shutdown_async()
            })
            .flatten();
            if let Some(future) = shutdown_future
                && let Err(payload) = AssertUnwindSafe(future).catch_unwind().await
            {
                diagnostics.error(
                    "shutdown_async",
                    task_id,
                    &FrameworkError::ActorPanic {
                        phase: "shutdown_async",
                        message: panic_message(payload.as_ref()),
                        task: task_id.get(),
                    },
                );
            }
        }
        let _ = guard(&diagnostics, task_id, "shutdown", || app.shutdown());
    });

    let snapshots = Snapshots {
        rx: views_rx,
        id: NEXT_CHANNEL_ID.fetch_add(1, Ordering::Relaxed),
    };
    (handle, snapshots, CoreTask { handle: task })
}

#[allow(deprecated)]
fn app_tick_interval<A: App>(app: &A) -> Option<Duration> {
    app.tick_interval()
}

#[allow(deprecated)]
fn app_tick<A: App>(app: &mut A) -> Command<A::Message> {
    app.tick()
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
    views_tx: &watch::Sender<(Arc<A::View>, u64)>,
    snapshot_sequence: &mut u64,
    active_subscriptions: &mut HashMap<u64, SubscriptionTask>,
    command_tx: &mpsc::Sender<InboxMessage<A::Message>>,
    phase: &'static str,
) -> bool {
    let Some(wanted) = guard(diagnostics, task, "tick_interval", || {
        app_tick_interval(app)
    }) else {
        return false;
    };

    if wanted != *interval_duration {
        *interval_duration = wanted;
        *ticker = wanted.map(make_ticker);
    }

    let Some(view) = guard(diagnostics, task, "view", || app.view()) else {
        return false;
    };

    *snapshot_sequence += 1;
    if diagnostics.debug_enabled() {
        diagnostics.event(
            EventKind::Snapshot,
            TARGET,
            phase,
            task,
            format!("{phase} #{snapshot_sequence}"),
        );
    }
    views_tx.send_replace((Arc::new(view), snapshot_sequence.saturating_sub(1)));

    if !reconcile_subscriptions(app, active_subscriptions, command_tx, diagnostics, task) {
        return false;
    }

    match guard(diagnostics, task, "should_quit", || app.should_quit()) {
        Some(quit) => !quit,
        None => false,
    }
}

fn reconcile_subscriptions<A: App>(
    app: &A,
    active: &mut HashMap<u64, SubscriptionTask>,
    tx: &mpsc::Sender<InboxMessage<A::Message>>,
    diagnostics: &Diagnostics,
    task: TaskId,
) -> bool {
    let Some(subscription) = guard(diagnostics, task, "subscriptions", || app.subscriptions())
    else {
        return false;
    };

    let mut wanted = HashSet::new();
    for entry in subscription.into_entries() {
        if !wanted.insert(entry.key) || active.contains_key(&entry.key) {
            continue;
        }

        let subscription_task = diagnostics.next_task();
        if diagnostics.debug_enabled() {
            diagnostics.event(
                EventKind::Subscription,
                TARGET,
                "subscription",
                subscription_task,
                format!("start {}", entry.key),
            );
        }

        let tx = tx.clone();
        let diagnostics = diagnostics.clone();
        let handle = tokio::spawn(async move {
            let mut stream = entry.stream;
            loop {
                match AssertUnwindSafe(stream.next()).catch_unwind().await {
                    Ok(Some(message)) => {
                        if tx
                            .send(InboxMessage::Subscription {
                                message,
                                task: subscription_task,
                            })
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Ok(None) => {
                        diagnostics.event(
                            EventKind::Subscription,
                            TARGET,
                            "subscription",
                            subscription_task,
                            format!("end {}", entry.key),
                        );
                        break;
                    }
                    Err(payload) => {
                        diagnostics.error(
                            "subscription",
                            subscription_task,
                            &FrameworkError::SubscriptionPanic {
                                phase: "subscription",
                                message: panic_message(payload.as_ref()),
                                task: subscription_task.get(),
                            },
                        );
                        let _ = tx
                            .send(InboxMessage::Fatal {
                                phase: "subscription",
                                task: subscription_task,
                            })
                            .await;
                        break;
                    }
                }
            }
        });
        active.insert(entry.key, SubscriptionTask { handle });
    }

    active.retain(|key, handle| {
        if wanted.contains(key) {
            true
        } else {
            if diagnostics.debug_enabled() {
                diagnostics.event(
                    EventKind::Subscription,
                    TARGET,
                    "subscription",
                    task,
                    format!("stop {key}"),
                );
            }
            handle.abort();
            false
        }
    });

    true
}

fn dispatch<M: Send + 'static>(
    command: Command<M>,
    commands: &mut JoinSet<()>,
    tx: &mpsc::Sender<InboxMessage<M>>,
    diagnostics: &Diagnostics,
) {
    for future in command.into_futures() {
        let command_task = diagnostics.next_task();
        diagnostics.event(
            EventKind::Command,
            TARGET,
            "command",
            command_task,
            "spawned",
        );
        let tx = tx.clone();
        let diagnostics = diagnostics.clone();
        let joined = AssertUnwindSafe(future).catch_unwind();

        commands.spawn(async move {
            match joined.await {
                Ok(message) => {
                    let _ = tx
                        .send(InboxMessage::Command {
                            message,
                            task: command_task,
                        })
                        .await;
                }
                Err(payload) => {
                    diagnostics.error(
                        "command",
                        command_task,
                        &FrameworkError::CommandPanic {
                            phase: "command",
                            message: panic_message(payload.as_ref()),
                            task: command_task.get(),
                        },
                    );
                    let _ = tx
                        .send(InboxMessage::Fatal {
                            phase: "command",
                            task: command_task,
                        })
                        .await;
                }
            }
        });
    }
}

fn reap_commands(commands: &mut JoinSet<()>, diagnostics: &Diagnostics, task: TaskId) {
    while let Some(result) = commands.try_join_next() {
        if let Err(error) = result {
            diagnostics.log(
                Level::Warn,
                TARGET,
                "command",
                task,
                &format!("command task ended abnormally: {error}"),
            );
        }
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

fn make_ticker(duration: Duration) -> Interval {
    let period = duration.max(Duration::from_millis(1));
    let mut ticker = interval_at(Instant::now() + period, period);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
    ticker
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
    use std::{
        collections::HashSet,
        sync::{
            Arc, Mutex,
            atomic::{AtomicUsize, Ordering as AtomicOrdering},
        },
        time::Duration,
    };

    use futures::stream;
    use tokio::time::timeout;

    use super::*;
    use crate::{Command, Subscription};

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
        assert_eq!(snapshots.revision(), 0);
        drop(handle);
        timeout(Duration::from_secs(1), task.join())
            .await
            .expect("actor exits");
    }

    #[tokio::test]
    async fn snapshot_revisions_advance_with_publications() {
        struct RevisionApp(u32);

        impl App for RevisionApp {
            type Message = ();
            type View = u32;

            fn update(&mut self, (): ()) -> Command<()> {
                self.0 += 1;
                Command::none()
            }

            fn view(&self) -> u32 {
                self.0
            }
        }

        let (handle, mut snapshots, task) = spawn(RevisionApp(0));
        assert_eq!(snapshots.revision(), 0);
        assert!(handle.send(()));

        let (view, revision) = timeout(Duration::from_secs(1), snapshots.changed_with_revision())
            .await
            .expect("snapshot arrives")
            .expect("actor alive");
        assert_eq!(*view, 1);
        assert_eq!(revision, 1);
        assert_eq!(snapshots.current_with_revision().1, 1);

        handle.shutdown();
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

    struct Burst {
        count: u32,
    }

    impl App for Burst {
        type Message = ();
        type View = u32;

        fn init(&mut self) -> Command<()> {
            Command::batch((0..INBOX_CAPACITY * 4).map(|_| Command::perform(async {}, |_| ())))
        }

        fn update(&mut self, (): ()) -> Command<()> {
            self.count += 1;
            Command::none()
        }

        fn view(&self) -> u32 {
            self.count
        }
    }

    #[tokio::test]
    async fn bounded_inbox_delivers_a_large_command_burst() {
        let options = RuntimeOptions::new(NonZeroUsize::new(1).expect("non-zero capacity"));
        let (handle, mut snapshots, task) = spawn_with_options(Burst { count: 0 }, options);

        timeout(Duration::from_secs(5), async {
            loop {
                if *snapshots.changed().await.expect("actor alive") == (INBOX_CAPACITY * 4) as u32 {
                    break;
                }
            }
        })
        .await
        .expect("every command result is delivered through backpressure");

        handle.shutdown();
        timeout(Duration::from_secs(1), task.join())
            .await
            .expect("actor exits");
    }

    struct SlowCommand {
        ran: Arc<AtomicUsize>,
    }

    impl App for SlowCommand {
        type Message = ();
        type View = usize;

        fn init(&mut self) -> Command<()> {
            let ran = self.ran.clone();
            Command::perform(
                async move {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    ran.fetch_add(1, AtomicOrdering::SeqCst);
                },
                |_| (),
            )
        }

        fn update(&mut self, (): ()) -> Command<()> {
            Command::none()
        }

        fn view(&self) -> usize {
            self.ran.load(AtomicOrdering::SeqCst)
        }
    }

    #[tokio::test]
    async fn shutdown_aborts_in_flight_commands() {
        let ran = Arc::new(AtomicUsize::new(0));
        let (handle, _snapshots, task) = spawn(SlowCommand { ran: ran.clone() });

        tokio::time::sleep(Duration::from_millis(30)).await;
        handle.shutdown();
        timeout(Duration::from_secs(1), task.join())
            .await
            .expect("actor exits");

        tokio::time::sleep(Duration::from_millis(250)).await;
        assert_eq!(
            ran.load(AtomicOrdering::SeqCst),
            0,
            "aborted command must not run"
        );
    }

    struct AsyncShutdown {
        order: Arc<Mutex<Vec<&'static str>>>,
    }

    impl App for AsyncShutdown {
        type Message = ();
        type View = ();

        fn update(&mut self, (): ()) -> Command<()> {
            Command::none()
        }

        fn view(&self) {}

        fn shutdown_async(
            &mut self,
        ) -> Option<std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>>> {
            let order = self.order.clone();
            Some(Box::pin(async move {
                tokio::time::sleep(Duration::from_millis(20)).await;
                order.lock().expect("order lock").push("async");
            }))
        }

        fn shutdown(&mut self) {
            self.order.lock().expect("order lock").push("sync");
        }
    }

    #[tokio::test]
    async fn async_shutdown_runs_before_sync_shutdown() {
        let order = Arc::new(Mutex::new(Vec::new()));
        let (handle, _snapshots, task) = spawn(AsyncShutdown {
            order: order.clone(),
        });

        handle.shutdown();
        timeout(Duration::from_secs(1), task.join())
            .await
            .expect("actor waits for async shutdown");

        assert_eq!(*order.lock().expect("order lock"), vec!["async", "sync"]);
    }

    struct PanickingAsyncShutdown {
        sync_ran: Arc<std::sync::atomic::AtomicBool>,
    }

    impl App for PanickingAsyncShutdown {
        type Message = ();
        type View = ();

        fn update(&mut self, (): ()) -> Command<()> {
            Command::none()
        }

        fn view(&self) {}

        fn shutdown_async(
            &mut self,
        ) -> Option<std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>>> {
            Some(Box::pin(async { panic!("async shutdown exploded") }))
        }

        fn shutdown(&mut self) {
            self.sync_ran
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }

    #[tokio::test]
    async fn async_shutdown_panic_is_recorded_and_sync_shutdown_still_runs() {
        let sync_ran = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let diagnostics = Diagnostics::new();
        let (handle, _snapshots, task) = spawn_with_diagnostics(
            PanickingAsyncShutdown {
                sync_ran: sync_ran.clone(),
            },
            diagnostics.clone(),
        );

        handle.shutdown();
        timeout(Duration::from_secs(1), task.join())
            .await
            .expect("actor exits");

        assert!(sync_ran.load(std::sync::atomic::Ordering::SeqCst));
        assert!(diagnostics.recent_errors().iter().any(|error| {
            error.code.as_deref() == Some("ferriso::actor::panic")
                && error.phase == "shutdown_async"
        }));
    }

    #[tokio::test]
    async fn aborting_the_actor_stops_in_flight_commands() {
        let ran = Arc::new(AtomicUsize::new(0));
        let (_handle, _snapshots, task) = spawn(SlowCommand { ran: ran.clone() });

        tokio::time::sleep(Duration::from_millis(30)).await;
        task.abort();

        tokio::time::sleep(Duration::from_millis(250)).await;
        assert_eq!(
            ran.load(AtomicOrdering::SeqCst),
            0,
            "dropping the actor must abort joined commands"
        );
    }

    #[tokio::test]
    async fn zero_tick_interval_is_clamped_instead_of_panicking() {
        struct ZeroTicker {
            ticks: u32,
        }

        impl App for ZeroTicker {
            type Message = ();
            type View = u32;

            fn update(&mut self, (): ()) -> Command<()> {
                Command::none()
            }

            fn view(&self) -> u32 {
                self.ticks
            }

            fn tick_interval(&self) -> Option<Duration> {
                Some(Duration::ZERO)
            }

            fn tick(&mut self) -> Command<()> {
                self.ticks += 1;
                Command::none()
            }
        }

        let (handle, mut snapshots, task) = spawn(ZeroTicker { ticks: 0 });
        timeout(Duration::from_secs(1), async {
            loop {
                if *snapshots.changed().await.expect("actor alive") >= 3 {
                    break;
                }
            }
        })
        .await
        .expect("zero interval ticks");
        handle.shutdown();
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
    async fn joining_an_aborted_actor_reports_the_failure() {
        let (_, _, task) = spawn(Counter::default());
        task.abort();
        assert!(task.join_checked().await.is_err());
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
        assert!(events.iter().all(|event| {
            matches!(
                event.kind,
                EventKind::Message | EventKind::Snapshot | EventKind::Lifecycle
            ) || event.task.get() > 1
        }));
        let command_tasks: HashSet<_> = events
            .iter()
            .filter(|event| matches!(event.kind, EventKind::Command | EventKind::CommandResult))
            .map(|event| event.task)
            .collect();
        assert_eq!(command_tasks.len(), 2, "each command gets its own task id");
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

    #[derive(Default)]
    struct Subscribed {
        total: u32,
    }

    enum SubscriptionMessage {
        Add(u32),
    }

    impl App for Subscribed {
        type Message = SubscriptionMessage;
        type View = u32;

        fn update(&mut self, message: SubscriptionMessage) -> Command<SubscriptionMessage> {
            match message {
                SubscriptionMessage::Add(value) => self.total += value,
            }
            Command::none()
        }

        fn view(&self) -> u32 {
            self.total
        }

        fn subscriptions(&self) -> Subscription<SubscriptionMessage> {
            Subscription::stream(
                7,
                stream::iter([SubscriptionMessage::Add(1), SubscriptionMessage::Add(2)]),
            )
        }
    }

    #[tokio::test]
    async fn subscriptions_feed_messages_into_the_actor() {
        let (handle, mut snapshots, task, diagnostics) = spawn_with_debug(Subscribed::default());

        timeout(Duration::from_secs(1), async {
            loop {
                if *snapshots.changed().await.expect("actor alive") == 3 {
                    break;
                }
            }
        })
        .await
        .expect("subscription messages arrive");

        assert!(
            diagnostics
                .events()
                .iter()
                .any(|event| event.kind == EventKind::Subscription)
        );

        timeout(Duration::from_secs(1), async {
            loop {
                if diagnostics
                    .events()
                    .iter()
                    .any(|event| event.detail == "end 7")
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("stream end is reported");

        handle.shutdown();
        timeout(Duration::from_secs(1), task.join())
            .await
            .expect("actor exits");
    }

    struct Timed {
        enabled: bool,
        fired: u32,
    }

    enum TimedMessage {
        Disable,
        Fire,
    }

    impl App for Timed {
        type Message = TimedMessage;
        type View = (bool, u32);

        fn update(&mut self, message: TimedMessage) -> Command<TimedMessage> {
            match message {
                TimedMessage::Disable => self.enabled = false,
                TimedMessage::Fire => self.fired += 1,
            }
            Command::none()
        }

        fn view(&self) -> (bool, u32) {
            (self.enabled, self.fired)
        }

        fn subscriptions(&self) -> Subscription<TimedMessage> {
            if self.enabled {
                Subscription::stream(
                    1,
                    stream::once(async {
                        tokio::time::sleep(Duration::from_millis(80)).await;
                        TimedMessage::Fire
                    }),
                )
            } else {
                Subscription::none()
            }
        }
    }

    #[tokio::test]
    async fn removing_a_subscription_aborts_its_stream() {
        let (handle, mut snapshots, task) = spawn(Timed {
            enabled: true,
            fired: 0,
        });

        assert!(handle.send(TimedMessage::Disable));
        timeout(Duration::from_secs(1), async {
            loop {
                let view = snapshots.changed().await.expect("actor alive");
                if !view.0 {
                    break;
                }
            }
        })
        .await
        .expect("disable is applied");

        tokio::time::sleep(Duration::from_millis(160)).await;
        assert_eq!(snapshots.current().1, 0, "aborted stream must not fire");

        handle.shutdown();
        timeout(Duration::from_secs(1), task.join())
            .await
            .expect("actor exits");
    }

    struct PanickingSubscription;

    impl App for PanickingSubscription {
        type Message = ();
        type View = bool;

        fn update(&mut self, (): ()) -> Command<()> {
            Command::none()
        }

        fn view(&self) -> bool {
            true
        }

        fn subscriptions(&self) -> Subscription<()> {
            Subscription::stream(1, stream::once(async { panic!("stream boom") }))
        }
    }

    #[tokio::test]
    async fn subscription_panic_is_reported_and_stops_the_actor() {
        let diagnostics = Diagnostics::new();
        let (handle, _snapshots, task) =
            spawn_with_diagnostics(PanickingSubscription, diagnostics.clone());

        timeout(Duration::from_secs(1), task.join())
            .await
            .expect("actor stops after a subscription panic");

        assert!(handle.is_closed());
        assert!(
            diagnostics
                .recent_errors()
                .iter()
                .any(|error| error.code.as_deref() == Some("ferriso::subscription::panic"))
        );
    }

    #[tokio::test]
    async fn command_panic_is_wrapped_and_stops_the_actor() {
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

        timeout(Duration::from_secs(1), task.join())
            .await
            .expect("actor stops after a command panic");

        assert!(handle.is_closed());
        let errors = diagnostics.recent_errors();
        let error = errors
            .iter()
            .find(|error| error.code.as_deref() == Some("ferriso::command::panic"))
            .expect("panic recorded");
        assert!(error.message.contains("command exploded"));
    }
}
