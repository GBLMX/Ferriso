//! GPUI adapter for Ferriso.
//!
//! GPUI owns the window, input routing, and element tree. This crate turns the
//! latest immutable core snapshot into a GPUI entity and forwards semantic
//! messages back to the actor. Application state never enters GPUI, so closing
//! or recreating a window cannot fork the domain model.
//!
//! A [`Renderer`] receives the current snapshot and a [`Ui`] sender. Keep
//! entity-local view state in the renderer; snapshots are immutable.

use std::{sync::Arc, time::Duration};

use ferriso_core::{App, CoreHandle, Snapshots};
use futures::{FutureExt, StreamExt, channel::mpsc};
use gpui::{
    AnyElement, App as GpuiApp, AppContext, Application, Bounds, Context, Render, Task,
    TitlebarOptions, Window, WindowBounds, WindowOptions, px, size,
};
use tokio::runtime::Builder;

pub use gpui;

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

/// GPUI-native translation of a view snapshot.
pub trait Renderer<V, M>: 'static {
    /// Build a native GPUI element tree for this snapshot.
    fn render(&mut self, view: &V, ui: &Ui<M>, window: &mut Window, cx: &mut GpuiApp)
    -> AnyElement;

    /// Observe a snapshot before its view is rendered.
    fn on_snapshot(&mut self, _view: &V) {}
}

/// Window settings supplied by the adapter caller, not by the core.
pub struct Settings {
    pub title: String,
    pub size: (f32, f32),
    pub application_id: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            title: "Ferriso".into(),
            size: (1100.0, 760.0),
            application_id: "io.github.gblmx.ferriso".into(),
        }
    }
}

struct Frontend<V, M, R> {
    view: Arc<V>,
    ui: Ui<M>,
    renderer: R,
    _bridge: Task<()>,
}

impl<V, M, R> Render for Frontend<V, M, R>
where
    V: 'static,
    M: 'static,
    R: Renderer<V, M>,
{
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let ui = self.ui.clone();
        let view = &self.view;
        self.renderer.render(view, &ui, window, cx)
    }
}

enum Bridge<V> {
    View(Arc<V>),
    Closed,
}

/// Spawn `app` on a private Tokio runtime and open a GPUI window.
///
/// Must run on the platform's main thread; GPUI owns the event loop.
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

/// Attach a GPUI window to an existing core actor.
///
/// The window owns a snapshot and a sender; the caller keeps actor lifecycle
/// ownership. Must run on the platform's main thread.
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
    let mut renderer = renderer;
    renderer.on_snapshot(&initial);

    let (mut receiver, relay) = relay_snapshots(snapshots);
    let ui = Ui { handle };
    let mut renderer = Some(renderer);

    Application::new().run(move |cx| {
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        let bounds = Bounds::centered(None, size(px(settings.size.0), px(settings.size.1)), cx);
        let titlebar = TitlebarOptions {
            title: Some(settings.title.clone().into()),
            ..Default::default()
        };
        let application_id = settings.application_id.clone();
        let renderer = renderer.take().expect("GPUI launched twice");
        let ui = ui.clone();
        let initial = initial.clone();

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(titlebar),
                app_id: Some(application_id),
                ..Default::default()
            },
            move |_window, cx| {
                cx.new(|cx| {
                    let bridge = cx.spawn(async move |this, cx| {
                        while let Some(event) = receiver.next().await {
                            match event {
                                Bridge::View(view) => {
                                    let updated =
                                        this.update(cx, |frontend: &mut Frontend<V, M, R>, cx| {
                                            frontend.renderer.on_snapshot(&view);
                                            frontend.view = view;
                                            cx.notify();
                                        });
                                    if updated.is_err() {
                                        break;
                                    }
                                }
                                Bridge::Closed => {
                                    let _ = this.update(cx, |_, cx| cx.quit());
                                    break;
                                }
                            }
                        }
                    });

                    Frontend {
                        view: initial,
                        ui: ui.clone(),
                        renderer,
                        _bridge: bridge,
                    }
                })
            },
        )
        .expect("failed to open the GPUI window");

        cx.activate(true);
    });

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

fn relay_snapshots<V>(mut snapshots: Snapshots<V>) -> (mpsc::UnboundedReceiver<Bridge<V>>, Relay)
where
    V: Send + Sync + 'static,
{
    let (tx, rx) = mpsc::unbounded();
    let (stop_tx, stop_rx) = futures::channel::oneshot::channel::<()>();
    let thread = std::thread::spawn(move || {
        futures::executor::block_on(async move {
            let relay = async move {
                while let Ok(view) = snapshots.changed().await {
                    if tx.unbounded_send(Bridge::View(view)).is_err() {
                        return false;
                    }
                }
                let _ = tx.unbounded_send(Bridge::Closed);
                true
            }
            .fuse();
            let stop = async move {
                let _ = stop_rx.await;
            }
            .fuse();
            futures::pin_mut!(relay, stop);

            futures::select! {
                _ = stop => {}
                _ = relay => {}
            }
        });
    });

    (
        rx,
        Relay {
            stop: Some(stop_tx),
            thread: Some(thread),
        },
    )
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
        let (mut rx, relay) = relay_snapshots(snapshots);

        handle.send(3);
        let event = futures::executor::block_on(rx.next()).expect("relayed snapshot");
        match event {
            Bridge::View(view) => assert_eq!(*view, 3),
            Bridge::Closed => panic!("actor closed unexpectedly"),
        }

        handle.shutdown();
        runtime.block_on(task.join());
        relay.stop();
    }
}
