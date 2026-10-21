//! Floem adapter for Ferriso.
//!
//! Floem owns the event loop, fine-grained signals, and the native view tree.
//! The application still lives in the core actor: this crate forwards immutable
//! snapshots into a Floem signal and exposes a sender instead of giving Floem
//! ownership of domain state.
//!
//! A [`Renderer`] is rebuilt for each snapshot inside a `dyn_container`. Keep
//! widget-local state (focus, scroll offsets, text drafts) in Floem signals held
//! by the renderer, not in the snapshot.

use std::{rc::Rc, sync::Arc, time::Duration};

use futures::FutureExt;

use ferriso_core::{App, CoreHandle, Snapshots};
use floem::{
    AnyView, Application, IntoView,
    ext_event::update_signal_from_channel,
    reactive::SignalGet,
    views::{Decorators, dyn_container, empty},
    window::WindowConfig,
};
use tokio::runtime::Builder;

pub use floem;

/// A cloneable handle for renderer callbacks that need to send semantic input.
pub struct Ui<M> {
    handle: CoreHandle<M>,
}

impl<M> Clone for Ui<M> {
    fn clone(&self) -> Self {
        Self {
            handle: self.handle.clone(),
        }
    }
}

impl<M> Ui<M> {
    /// Send one semantic message to the core actor.
    pub fn send(&self, message: M) -> bool {
        self.handle.send(message)
    }

    /// Whether the core actor has stopped.
    pub fn is_closed(&self) -> bool {
        self.handle.is_closed()
    }
}

/// Floem-native translation of a view snapshot.
pub trait Renderer<V, M>: 'static {
    /// Build a native Floem view for this snapshot.
    fn view(&self, view: &V, ui: &Ui<M>) -> AnyView;

    /// Observe a snapshot before its view is built.
    fn on_snapshot(&self, _view: &V) {}
}

/// Window settings supplied by the adapter caller, not by the core.
pub struct Settings {
    pub title: String,
    pub size: (f32, f32),
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            title: "Ferriso".into(),
            size: (1100.0, 760.0),
        }
    }
}

/// Spawn `app` on a private Tokio runtime and open a Floem window.
///
/// Must run on the platform's main thread; Floem owns the event loop.
pub fn run<A, R>(app: A, renderer: R, settings: Settings)
where
    A: App,
    R: Renderer<A::View, A::Message>,
{
    let runtime = Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to start Tokio runtime for the core actor");
    let (handle, snapshots, task) = {
        let _enter = runtime.enter();
        ferriso_core::spawn(app)
    };

    run_with(handle.clone(), snapshots, renderer, settings);

    handle.shutdown();
    let _ =
        runtime.block_on(async { tokio::time::timeout(Duration::from_secs(1), task.join()).await });
}

/// Spawn an application built inside the adapter's Tokio runtime.
///
/// Must run on the platform's main thread.
pub fn run_with_app<A, R, F>(build: F, renderer: R, settings: Settings)
where
    A: App,
    R: Renderer<A::View, A::Message>,
    F: FnOnce() -> A,
{
    let runtime = Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to start Tokio runtime for the core actor");
    let (handle, snapshots, task) = {
        let _enter = runtime.enter();
        ferriso_core::spawn(build())
    };

    run_with(handle.clone(), snapshots, renderer, settings);

    handle.shutdown();
    let _ =
        runtime.block_on(async { tokio::time::timeout(Duration::from_secs(1), task.join()).await });
}

/// Attach a Floem window to an existing core actor.
///
/// The window owns a snapshot signal and a sender; the caller keeps actor
/// lifecycle ownership. Must run on the platform's main thread.
pub fn run_with<V, M, R>(
    handle: CoreHandle<M>,
    snapshots: Snapshots<V>,
    renderer: R,
    settings: Settings,
) where
    V: Send + Sync + 'static,
    M: Send + 'static,
    R: Renderer<V, M>,
{
    let initial = snapshots.current();
    let (tx, rx) = crossbeam_channel::unbounded();

    // Constructing the application installs the event-loop proxy used by
    // `quit_app` when the actor closes.
    let application = Application::new();
    let relay = relay_snapshots(snapshots, tx);

    let ui = Ui { handle };
    let renderer = Rc::new(renderer);
    let config = WindowConfig::default()
        .title(settings.title)
        .size((settings.size.0 as f64, settings.size.1 as f64));

    application
        .window(
            move |_| {
                let (read, write) = floem::reactive::create_signal(Some(initial));
                update_signal_from_channel(write, rx);

                let ui = ui.clone();
                let renderer = renderer.clone();
                dyn_container(
                    move || read.get(),
                    move |snapshot| match snapshot {
                        Some(view) => {
                            renderer.on_snapshot(&view);
                            renderer.view(&view, &ui)
                        }
                        None => empty().into_any(),
                    },
                )
                .style(|style| style.size_full())
            },
            Some(config),
        )
        .run();

    relay.stop();
}

struct Relay {
    stop: Option<futures::channel::oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Relay {
    fn stop(mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Relay {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn relay_snapshots<V>(mut snapshots: Snapshots<V>, tx: crossbeam_channel::Sender<Arc<V>>) -> Relay
where
    V: Send + Sync + 'static,
{
    let (stop_tx, stop_rx) = futures::channel::oneshot::channel::<()>();
    let thread = std::thread::spawn(move || {
        futures::executor::block_on(async move {
            let relay = async move {
                while let Ok(view) = snapshots.changed().await {
                    if tx.send(view).is_err() {
                        return false;
                    }
                }
                true
            }
            .fuse();
            let stop = async move {
                let _ = stop_rx.await;
            }
            .fuse();
            futures::pin_mut!(relay, stop);

            let actor_closed = futures::select! {
                _ = stop => false,
                closed = relay => closed,
            };
            if actor_closed {
                floem::quit_app();
            }
        });
    });

    Relay {
        stop: Some(stop_tx),
        thread: Some(thread),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferriso_core::Command;

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

    #[test]
    fn relay_forwards_actor_snapshots() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let (handle, snapshots, task) = {
            let _enter = runtime.enter();
            ferriso_core::spawn(Counter(0))
        };
        let (tx, rx) = crossbeam_channel::unbounded();
        let relay = relay_snapshots(snapshots, tx);

        handle.send(3);
        let view = rx
            .recv_timeout(Duration::from_secs(2))
            .expect("relayed snapshot");
        assert_eq!(*view, 3);

        handle.shutdown();
        runtime.block_on(task.join());
        relay.stop();
    }
}
