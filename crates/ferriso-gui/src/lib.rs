//! Iced adapter for Ferriso.
//!
//! Iced still drives its own window lifecycle, redraws, and widget tree. What it
//! does **not** own is the application: the domain state lives in the core
//! actor. The iced `State` here is only the latest immutable snapshot plus a
//! sender, so recreating a window cannot fork the application state.

use std::{
    pin::Pin,
    sync::{Arc, Mutex},
    time::Duration,
};

use ferriso_core::{App, CoreHandle, Snapshots};
use futures::{Stream, stream};
use iced::{Element, Subscription, Task};

pub use iced;

/// Messages handled by the iced adapter.
pub enum Message<V, M> {
    /// Semantic input produced by the application's iced view.
    User(M),
    /// A newer immutable view snapshot.
    Snapshot(Arc<V>),
    /// The core actor stopped; close the window.
    Closed,
}

impl<V, M: Clone> Clone for Message<V, M> {
    fn clone(&self) -> Self {
        match self {
            Self::User(message) => Self::User(message.clone()),
            Self::Snapshot(view) => Self::Snapshot(view.clone()),
            Self::Closed => Self::Closed,
        }
    }
}

/// Iced-native translation of a view snapshot into widgets.
pub trait Renderer<V, M> {
    /// Lay the snapshot out with iced widgets. Focus, text editing, scrolling,
    /// and animation state stay inside iced's widget tree.
    fn view<'a>(&self, view: &'a V) -> Element<'a, Message<V, M>>;

    /// Iced subscriptions owned by the view layer (keyboard shortcuts, window
    /// events, animations). Snapshot deliveries are added by the adapter.
    fn subscription(&self, _view: &V) -> Subscription<Message<V, M>> {
        Subscription::none()
    }
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

struct Frontend<V, M, R> {
    view: Arc<V>,
    handle: CoreHandle<M>,
    renderer: R,
}

/// Spawn `app` on a private Tokio runtime and open an iced window.
///
/// Use [`run_with`] to attach a window to a core actor created elsewhere.
pub fn run<A, R>(app: A, renderer: R, settings: Settings) -> iced::Result
where
    A: App,
    A::Message: Clone,
    R: Renderer<A::View, A::Message> + Send + 'static,
{
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to start Tokio runtime for the core actor");
    let (handle, snapshots, task) = {
        let _enter = runtime.enter();
        ferriso_core::spawn(app)
    };

    let result = run_with::<A, R>(handle.clone(), snapshots, renderer, settings);

    handle.shutdown();
    let _ =
        runtime.block_on(async { tokio::time::timeout(Duration::from_secs(1), task.join()).await });
    result
}

/// Attach an iced window to an existing core actor.
///
/// The window owns only a snapshot and a sender. Closing it does not stop a
/// core that other frontends still use.
pub fn run_with<A, R>(
    handle: CoreHandle<A::Message>,
    snapshots: Snapshots<A::View>,
    renderer: R,
    settings: Settings,
) -> iced::Result
where
    A: App,
    A::Message: Clone,
    R: Renderer<A::View, A::Message> + Send + 'static,
{
    let frontend = Frontend {
        view: snapshots.current(),
        handle,
        renderer,
    };
    let boot = Mutex::new(Some(frontend));
    let title = settings.title;
    let size = settings.size;
    let subscription_snapshots = snapshots.clone();

    iced::application(
        move || {
            let frontend = boot
                .lock()
                .expect("boot lock poisoned")
                .take()
                .expect("iced booted more than once");
            (frontend, Task::none())
        },
        update::<A, R>,
        view::<A, R>,
    )
    .title(move |_: &Frontend<A::View, A::Message, R>| title.clone())
    .subscription(move |frontend: &Frontend<A::View, A::Message, R>| {
        Subscription::batch([
            subscription(subscription_snapshots.clone()),
            frontend.renderer.subscription(&frontend.view),
        ])
    })
    .window_size(size)
    .run()
}

fn update<A, R>(
    frontend: &mut Frontend<A::View, A::Message, R>,
    message: Message<A::View, A::Message>,
) -> Task<Message<A::View, A::Message>>
where
    A: App,
    R: Renderer<A::View, A::Message>,
{
    match message {
        Message::User(message) => {
            if frontend.handle.send(message) {
                Task::none()
            } else {
                iced::exit()
            }
        }
        Message::Snapshot(view) => {
            frontend.view = view;
            Task::none()
        }
        Message::Closed => iced::exit(),
    }
}

fn view<A, R>(
    frontend: &Frontend<A::View, A::Message, R>,
) -> Element<'_, Message<A::View, A::Message>>
where
    A: App,
    R: Renderer<A::View, A::Message>,
{
    frontend.renderer.view(&frontend.view)
}

fn subscription<V, M>(snapshots: Snapshots<V>) -> Subscription<Message<V, M>>
where
    V: Send + Sync + 'static,
    M: Send + 'static,
{
    Subscription::run_with(snapshots, snapshot_stream::<V>).map(map_snapshot::<V, M>)
}

enum SnapshotEvent<V> {
    Snapshot(Arc<V>),
    Closed,
}

type SnapshotStream<V> = Pin<Box<dyn Stream<Item = SnapshotEvent<V>> + Send + 'static>>;

fn snapshot_stream<V>(snapshots: &Snapshots<V>) -> SnapshotStream<V>
where
    V: Send + Sync + 'static,
{
    let snapshots = snapshots.clone();
    Box::pin(stream::unfold(
        (snapshots, false),
        |(mut snapshots, done)| async move {
            if done {
                return None;
            }
            match snapshots.changed().await {
                Ok(view) => Some((SnapshotEvent::Snapshot(view), (snapshots, false))),
                Err(_) => Some((SnapshotEvent::Closed, (snapshots, true))),
            }
        },
    ))
}

fn map_snapshot<V, M>(event: SnapshotEvent<V>) -> Message<V, M> {
    match event {
        SnapshotEvent::Snapshot(view) => Message::Snapshot(view),
        SnapshotEvent::Closed => Message::Closed,
    }
}
