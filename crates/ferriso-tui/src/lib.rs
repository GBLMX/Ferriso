//! Ratatui adapter for Ferriso.
//!
//! The adapter owns the terminal, the blocking event loop, and the Tokio
//! runtime that hosts the core actor. Applications own their terminal-local
//! state (focus, scroll offsets, an in-progress text edit) inside a
//! [`Renderer`] and send semantic messages; they never touch core state.
//!
//! The adapter reserves no quit chord. `Ctrl+Q` and `Ctrl+C` reach the
//! renderer, so an application can run its save guard or use `Ctrl+C` for
//! copy; return [`Outcome::Quit`] when the application decides to leave.

use std::{
    io::{self, stdout},
    sync::Arc,
    time::Duration,
};

use crossterm::event::{DisableMouseCapture, EnableMouseCapture, Event, EventStream};
use ferriso_core::{App, CoreContext, Diagnostics, Snapshots};
use futures::{StreamExt, stream};
use ratatui::Frame;

pub mod keymap;

pub use crossterm;
pub use ratatui;

/// What the terminal adapter should do after a renderer handled an event.
pub enum Outcome<M> {
    /// The renderer consumed the event without producing a message.
    Ignored,
    /// Send this semantic message to the core actor.
    Message(M),
    /// Leave the terminal event loop.
    Quit,
}

/// Terminal-specific drawing and input translation.
pub trait Renderer<V, M> {
    /// Draw the latest snapshot. Layout and styling belong here, not in core.
    fn draw(&mut self, view: &V, frame: &mut Frame);

    /// Translate one terminal event into zero or one semantic message.
    fn handle(&mut self, view: &V, event: Event) -> Outcome<M>;

    /// Observe a new snapshot before it is drawn.
    ///
    /// Called once for the boot snapshot and again whenever the actor
    /// publishes a different snapshot. Use this hook for cache invalidation,
    /// notification diffing, and other bookkeeping; it replaces doing the same
    /// work in both [`Renderer::draw`] and [`Renderer::handle`].
    fn on_snapshot(&mut self, _view: &V) {}

    /// Optional message to send after a snapshot changed the viewport.
    ///
    /// The terminal adapter has no return value from [`Renderer::draw`], so
    /// layouts whose height depends on snapshot state (an open search bar, for
    /// example) use this hook to publish a new [`crate::Renderer`] viewport.
    /// Return `None` when nothing changed; returning a message on every
    /// snapshot creates a feedback loop.
    fn on_snapshot_message(&mut self, _view: &V) -> Option<M> {
        None
    }
}

/// Adapter options.
pub struct Options {
    /// Capture mouse events. Keyboard-only applications can leave this off.
    pub mouse: bool,
    /// Reserved for adapters that poll for input.
    ///
    /// The built-in loop waits on crossterm's event stream, so snapshots and
    /// keys wake it immediately instead of waiting for this interval.
    pub poll_interval: Duration,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            mouse: false,
            poll_interval: Duration::from_millis(50),
        }
    }
}

/// Mouse capture enabled by the adapter. Ratatui's panic hook restores raw
/// mode and the alternate screen, but it does not know about crossterm mouse
/// capture, so this guard owns that extra terminal state.
struct MouseCapture {
    active: bool,
}

impl MouseCapture {
    fn enable() -> io::Result<Self> {
        crossterm::execute!(stdout(), EnableMouseCapture)?;
        Ok(Self { active: true })
    }

    fn disable(&mut self) {
        if self.active {
            let _ = crossterm::execute!(stdout(), DisableMouseCapture);
            self.active = false;
        }
    }
}

impl Drop for MouseCapture {
    fn drop(&mut self) {
        self.disable();
    }
}

/// Spawn `app` on a private Tokio runtime and run it on the terminal.
///
/// The runtime, the actor, and the terminal all have the same lifetime. Use
/// [`run_with`] to attach the terminal to a core actor created elsewhere.
pub fn run<A, R>(app: A, renderer: R, options: Options) -> io::Result<()>
where
    A: App,
    R: Renderer<A::View, A::Message>,
{
    run_with_app(move || app, renderer, options)
}

/// Spawn an application built inside the adapter's Tokio runtime.
///
/// Use this when construction itself needs a reactor (HTTP clients, audio
/// devices, `tokio::spawn`). [`run`] cannot help there: its runtime is created
/// after the caller has already built the app.
pub fn run_with_app<A, R, F>(build: F, renderer: R, options: Options) -> io::Result<()>
where
    A: App,
    R: Renderer<A::View, A::Message>,
    F: FnOnce() -> A,
{
    run_with_app_diagnostics(build, renderer, options, Diagnostics::new())
}

/// Like [`run_with_app`], but exposes the started actor to a composition hook.
///
/// The hook runs after the actor exists and before the terminal opens. Use it
/// to start IPC, MPRIS, or other sidecars that need the real [`CoreContext`].
/// The adapter still owns the runtime and shuts the actor down after the loop.
pub fn run_with_app_context<A, R, F, S>(
    build: F,
    start: S,
    renderer: R,
    options: Options,
) -> io::Result<()>
where
    A: App,
    R: Renderer<A::View, A::Message>,
    F: FnOnce() -> A,
    S: FnOnce(CoreContext<A::Message, A::View>),
{
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let (handle, snapshots, task) = {
        let _enter = runtime.enter();
        ferriso_core::spawn(build())
    };
    {
        let _enter = runtime.enter();
        start(CoreContext::new(handle.clone(), snapshots.clone()));
    }

    let result = run_with::<A, R>(handle.clone(), snapshots, renderer, options);

    handle.shutdown();
    let _ =
        runtime.block_on(async { tokio::time::timeout(Duration::from_secs(1), task.join()).await });
    result
}

/// Like [`run_with_app`], but starts the actor with caller-owned diagnostics.
///
/// The same handle can be installed as a logger, included in an overlay, or
/// exported after the terminal exits. Actor panics are recorded here instead
/// of disappearing into the quiet `spawn` path.
pub fn run_with_app_diagnostics<A, R, F>(
    build: F,
    renderer: R,
    options: Options,
    diagnostics: Diagnostics,
) -> io::Result<()>
where
    A: App,
    R: Renderer<A::View, A::Message>,
    F: FnOnce() -> A,
{
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let (handle, snapshots, task) = {
        let _enter = runtime.enter();
        ferriso_core::spawn_with_diagnostics(build(), diagnostics)
    };

    let result = run_with::<A, R>(handle.clone(), snapshots, renderer, options);

    handle.shutdown();
    let _ =
        runtime.block_on(async { tokio::time::timeout(Duration::from_secs(1), task.join()).await });
    result
}

/// Attach the terminal to an existing core actor.
///
/// This is the multi-frontend path: one actor can publish snapshots to a TUI,
/// an iced window, or a test harness. The adapter does not shut the actor down
/// when it exits; the caller owns that lifecycle.
pub fn run_with<A, R>(
    handle: ferriso_core::CoreHandle<A::Message>,
    snapshots: ferriso_core::Snapshots<A::View>,
    renderer: R,
    options: Options,
) -> io::Result<()>
where
    A: App,
    R: Renderer<A::View, A::Message>,
{
    let mut terminal = ratatui::try_init()?;
    let mut mouse = if options.mouse {
        match MouseCapture::enable() {
            Ok(capture) => Some(capture),
            Err(error) => {
                let _ = ratatui::try_restore();
                return Err(error);
            }
        }
    } else {
        None
    };

    let result = event_loop::<A, R>(&mut terminal, renderer, &handle, &snapshots);

    if let Some(capture) = mouse.as_mut() {
        capture.disable();
    }
    let restore = ratatui::try_restore();
    match (result, restore) {
        (Err(error), _) => Err(error),
        (Ok(()), Err(error)) => Err(error),
        (Ok(()), Ok(())) => Ok(()),
    }
}

fn event_loop<A, R>(
    terminal: &mut ratatui::DefaultTerminal,
    mut renderer: R,
    handle: &ferriso_core::CoreHandle<A::Message>,
    snapshots: &ferriso_core::Snapshots<A::View>,
) -> io::Result<()>
where
    A: App,
    R: Renderer<A::View, A::Message>,
{
    enum LoopEvent<V> {
        Terminal(io::Result<Event>),
        Snapshot(Arc<V>),
        Closed,
    }

    let mut view = boot_snapshot(snapshots, &mut renderer);
    if let Some(message) = renderer.on_snapshot_message(&view)
        && !handle.send(message)
    {
        return Ok(());
    }
    terminal.draw(|frame| renderer.draw(&view, frame))?;

    let terminal_events = EventStream::new().map(LoopEvent::Terminal);
    let snapshot_events = stream::unfold(
        (snapshots.clone(), false),
        |(mut snapshots, done)| async move {
            if done {
                return None;
            }
            match snapshots.changed().await {
                Ok(view) => Some((LoopEvent::Snapshot(view), (snapshots, false))),
                Err(_) => Some((LoopEvent::Closed, (snapshots, true))),
            }
        },
    );
    let events = stream::select(terminal_events, snapshot_events);
    futures::pin_mut!(events);

    // One `block_on` for the whole loop. Re-entering the executor per event
    // would give crossterm's event-stream waker a different task each time,
    // and a stale wake could then leave the loop asleep.
    futures::executor::block_on(async {
        while let Some(event) = events.next().await {
            let dirty = match event {
                LoopEvent::Terminal(Ok(terminal_event)) => {
                    match renderer.handle(&view, terminal_event) {
                        Outcome::Ignored => true,
                        Outcome::Message(message) => {
                            if !handle.send(message) {
                                break;
                            }
                            true
                        }
                        Outcome::Quit => break,
                    }
                }
                LoopEvent::Terminal(Err(error)) => return Err(error),
                LoopEvent::Snapshot(latest) => {
                    if Arc::ptr_eq(&view, &latest) {
                        false
                    } else {
                        view = latest;
                        renderer.on_snapshot(&view);
                        if let Some(message) = renderer.on_snapshot_message(&view)
                            && !handle.send(message)
                        {
                            break;
                        }
                        true
                    }
                }
                LoopEvent::Closed => break,
            };

            if dirty {
                let changed = sync_snapshot(&mut view, snapshots, &mut renderer);
                if changed
                    && let Some(message) = renderer.on_snapshot_message(&view)
                    && !handle.send(message)
                {
                    break;
                }
                terminal.draw(|frame| renderer.draw(&view, frame))?;
            }
        }
        Ok(())
    })
}

/// Read the boot snapshot and notify the renderer before the first draw.
fn boot_snapshot<V, M, R>(snapshots: &Snapshots<V>, renderer: &mut R) -> Arc<V>
where
    R: Renderer<V, M>,
{
    let view = snapshots.current();
    renderer.on_snapshot(&view);
    view
}

/// Store a newer snapshot and run the renderer hook exactly once.
fn sync_snapshot<V, M, R>(view: &mut Arc<V>, snapshots: &Snapshots<V>, renderer: &mut R) -> bool
where
    R: Renderer<V, M>,
{
    let latest = snapshots.current();
    if Arc::ptr_eq(view, &latest) {
        return false;
    }
    *view = latest;
    renderer.on_snapshot(view);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferriso_core::{App, Command};

    struct Counter(u32);

    impl App for Counter {
        type Message = u32;
        type View = u32;

        fn update(&mut self, message: u32) -> Command<u32> {
            self.0 += message;
            Command::none()
        }

        fn view(&self) -> u32 {
            self.0
        }
    }

    #[derive(Default)]
    struct Hooked {
        seen: Vec<u32>,
    }

    impl Renderer<u32, u32> for Hooked {
        fn draw(&mut self, _view: &u32, _frame: &mut Frame) {}

        fn handle(&mut self, _view: &u32, _event: Event) -> Outcome<u32> {
            Outcome::Ignored
        }

        fn on_snapshot(&mut self, view: &u32) {
            self.seen.push(*view);
        }
    }

    #[test]
    fn snapshot_hooks_cover_boot_and_changes() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("test runtime");
        let (handle, snapshots, task) = {
            let _enter = runtime.enter();
            ferriso_core::spawn(Counter(0))
        };

        let mut renderer = Hooked::default();
        let mut view = boot_snapshot(&snapshots, &mut renderer);
        assert_eq!(renderer.seen, vec![0]);

        handle.send(1);
        let mut wait = snapshots.clone();
        runtime
            .block_on(async { tokio::time::timeout(Duration::from_secs(1), wait.changed()).await })
            .expect("snapshot arrives")
            .expect("actor alive");

        assert!(sync_snapshot(&mut view, &snapshots, &mut renderer));
        assert_eq!(renderer.seen, vec![0, 1]);
        assert!(!sync_snapshot(&mut view, &snapshots, &mut renderer));

        handle.shutdown();
        runtime
            .block_on(async { tokio::time::timeout(Duration::from_secs(1), task.join()).await })
            .expect("actor exits");
    }
}
