//! Ratatui adapter for Ferriso.
//!
//! The adapter owns the terminal, the blocking event loop, and the Tokio
//! runtime that hosts the core actor. Applications own their terminal-local
//! state (focus, scroll offsets, an in-progress text edit) inside a
//! [`Renderer`] and send semantic messages; they never touch core state.

use std::{
    io::{self, stdout},
    time::Duration,
};

use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers,
};
use ferriso_core::App;
use ratatui::Frame;

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
}

/// Adapter options.
pub struct Options {
    /// Capture mouse events. Keyboard-only applications can leave this off.
    pub mouse: bool,
    /// Maximum time to block waiting for a terminal event.
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

/// Spawn `app` on a private Tokio runtime and run it on the terminal.
///
/// The runtime, the actor, and the terminal all have the same lifetime. Use
/// [`run_with`] to attach the terminal to a core actor created elsewhere.
pub fn run<A, R>(app: A, renderer: R, options: Options) -> io::Result<()>
where
    A: App,
    R: Renderer<A::View, A::Message>,
{
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let (handle, snapshots, task) = {
        let _enter = runtime.enter();
        ferriso_core::spawn(app)
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
    let mut terminal = ratatui::init();
    if options.mouse {
        crossterm::execute!(stdout(), EnableMouseCapture)?;
    }

    let result = event_loop::<A, R>(&mut terminal, renderer, &handle, &snapshots, &options);

    if options.mouse {
        let _ = crossterm::execute!(stdout(), DisableMouseCapture);
    }
    ratatui::restore();
    result
}

fn event_loop<A, R>(
    terminal: &mut ratatui::DefaultTerminal,
    mut renderer: R,
    handle: &ferriso_core::CoreHandle<A::Message>,
    snapshots: &ferriso_core::Snapshots<A::View>,
    options: &Options,
) -> io::Result<()>
where
    A: App,
    R: Renderer<A::View, A::Message>,
{
    let mut view = snapshots.current();
    terminal.draw(|frame| renderer.draw(&view, frame))?;

    loop {
        if snapshots.closed() {
            break;
        }

        let mut dirty = !std::sync::Arc::ptr_eq(&view, &snapshots.current());

        if event::poll(options.poll_interval)? {
            let terminal_event = event::read()?;
            if quit_shortcut(&terminal_event) {
                break;
            }
            dirty = true;
            match renderer.handle(&view, terminal_event) {
                Outcome::Ignored => {}
                Outcome::Message(message) => {
                    if !handle.send(message) {
                        break;
                    }
                }
                Outcome::Quit => break,
            }
        }

        if dirty {
            view = snapshots.current();
            terminal.draw(|frame| renderer.draw(&view, frame))?;
        }
    }
    Ok(())
}

fn quit_shortcut(event: &Event) -> bool {
    let Event::Key(key) = event else {
        return false;
    };
    key.kind.is_press()
        && key.modifiers.contains(KeyModifiers::CONTROL)
        && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('q'))
}
