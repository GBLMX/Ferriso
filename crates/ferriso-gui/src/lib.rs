//! Iced adapter for Ferriso.
//!
//! Iced still drives its own window lifecycle, redraws, and widget tree. What it
//! does **not** own is the application: the domain state lives in the core
//! actor. The iced `State` here is only the latest immutable snapshot plus a
//! sender, so recreating a window cannot fork the application state.

use std::{
    marker::PhantomData,
    pin::Pin,
    sync::{Arc, Mutex},
    time::Duration,
};

pub mod art;
pub mod design;
pub mod interaction;
pub mod keymap;
pub mod motion;

use ferriso_core::{App, CoreHandle, Diagnostics, Snapshots};
use futures::{Stream, stream};
use iced::{Element, Subscription, Task, Theme};

pub use iced;

/// Stable desktop application id.
///
/// On Wayland this becomes `xdg_toplevel.set_app_id`; on X11 it becomes both
/// `WM_CLASS` parts. KDE Plasma uses it to associate the window with
/// `packaging/io.github.gblmx.ferriso.desktop` (icon, taskbar grouping, window
/// rules), so the two names must stay identical.
pub const APPLICATION_ID: &str = "io.github.gblmx.ferriso";

/// Messages handled by the iced adapter.
pub enum Message<V, M, L = ()> {
    /// Semantic input produced by the application's iced view.
    User(M),
    /// Renderer-local input (seek preview, text focus, a key that needs local
    /// state) that is handled without waking the core actor.
    Local(L),
    /// A newer immutable view snapshot.
    Snapshot(Arc<V>),
    /// The core actor stopped; close the window.
    Closed,
}

impl<V, M: Clone, L: Clone> Clone for Message<V, M, L> {
    fn clone(&self) -> Self {
        match self {
            Self::User(message) => Self::User(message.clone()),
            Self::Local(local) => Self::Local(local.clone()),
            Self::Snapshot(view) => Self::Snapshot(view.clone()),
            Self::Closed => Self::Closed,
        }
    }
}

/// Iced-native translation of a view snapshot into widgets.
pub trait Renderer<V, M, L = ()> {
    /// Lay the snapshot out with iced widgets. Focus, text editing, scrolling,
    /// and animation state stay inside iced's widget tree.
    fn view<'a>(&'a self, view: &'a V) -> Element<'a, Message<V, M, L>>;

    /// Iced subscriptions owned by the view layer (keyboard shortcuts, window
    /// events, animations). Snapshot deliveries are added by the adapter.
    fn subscription(&self, _view: &V) -> Subscription<Message<V, M, L>> {
        Subscription::none()
    }

    /// Handle frontend-local input and optionally produce follow-up messages.
    fn update(&mut self, _local: L, _view: &V) -> Task<Message<V, M, L>> {
        Task::none()
    }

    /// Observe a semantic message before it is sent to the core. Use this for
    /// renderer-local state that mirrors a message, such as a seek preview.
    fn on_message(&mut self, _message: &M) {}

    /// Observe a new snapshot before it replaces the previous one. Cover and
    /// QR caches, notifications, and other view-local bookkeeping belong here.
    ///
    /// This hook has no way to return an iced [`Task`]. Use
    /// [`Renderer::on_snapshot_task`] when snapshot handling also needs to
    /// focus a widget or start renderer-local work.
    fn on_snapshot(&mut self, _view: &V) {}

    /// Observe a snapshot and schedule renderer-local work.
    ///
    /// Called once for the boot snapshot and again for every later snapshot,
    /// before the adapter stores it. The default forwards to
    /// [`Renderer::on_snapshot`] and returns no task, so existing bookkeeping
    /// hooks keep working. Override this method instead of `on_snapshot` when
    /// the snapshot should focus or operate a widget.
    fn on_snapshot_task(&mut self, view: &V) -> Task<Message<V, M, L>> {
        self.on_snapshot(view);
        Task::none()
    }

    /// The window theme for this snapshot. The default follows the system.
    fn theme(&self, _view: &V) -> Theme {
        Theme::Dark
    }

    /// The window title for this snapshot.
    ///
    /// Returning `None` keeps [`Settings::title`].
    fn title(&self, _view: &V) -> Option<String> {
        None
    }
}

/// Window settings supplied by the adapter caller, not by the core.
pub struct Settings {
    pub title: String,
    pub size: (f32, f32),
    /// Whether the window manager may close the window directly.
    ///
    /// Set this to `false` when the renderer needs to intercept close
    /// requests, for example to prompt about unsaved work.
    pub exit_on_close_request: bool,
    /// Desktop application identity.
    ///
    /// On Wayland this is `xdg_toplevel.set_app_id`; on X11 it is `WM_CLASS`.
    /// It must match the application's desktop file on KDE Plasma.
    pub application_id: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            title: "Ferriso".into(),
            size: (1100.0, 760.0),
            exit_on_close_request: true,
            application_id: APPLICATION_ID.into(),
        }
    }
}

struct Frontend<V, M, L, R> {
    view: Arc<V>,
    handle: CoreHandle<M>,
    renderer: R,
    fallback_title: String,
    _local: PhantomData<L>,
}

impl<V, M, L, R> Frontend<V, M, L, R>
where
    R: Renderer<V, M, L>,
{
    fn new(
        view: Arc<V>,
        handle: CoreHandle<M>,
        renderer: R,
        fallback_title: String,
    ) -> (Self, Task<Message<V, M, L>>) {
        let mut frontend = Self {
            view,
            handle,
            renderer,
            fallback_title,
            _local: PhantomData,
        };
        let task = frontend.renderer.on_snapshot_task(&frontend.view);
        (frontend, task)
    }

    fn apply_snapshot(&mut self, view: Arc<V>) -> Task<Message<V, M, L>> {
        let task = self.renderer.on_snapshot_task(&view);
        self.view = view;
        task
    }
}

/// Spawn `app` on a private Tokio runtime and open an iced window.
///
/// Applications whose constructor needs a Tokio reactor should use
/// [`run_with_app`].
///
/// Use [`run_with`] to attach a window to a core actor created elsewhere.
pub fn run<A, L, R>(app: A, renderer: R, settings: Settings) -> iced::Result
where
    A: App,
    A::Message: Clone,
    L: Send + 'static,
    R: Renderer<A::View, A::Message, L> + Send + 'static,
{
    run_with_app(move || app, renderer, settings)
}

/// Spawn an application built inside the adapter's Tokio runtime.
///
/// Use this when construction itself needs a reactor (HTTP clients, audio
/// devices, `tokio::spawn`). [`run`] cannot help there: its runtime is created
/// after the caller has already built the app.
pub fn run_with_app<A, L, R, F>(build: F, renderer: R, settings: Settings) -> iced::Result
where
    A: App,
    A::Message: Clone,
    L: Send + 'static,
    R: Renderer<A::View, A::Message, L> + Send + 'static,
    F: FnOnce() -> A,
{
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to start Tokio runtime for the core actor");
    let (handle, snapshots, task) = {
        let _enter = runtime.enter();
        ferriso_core::spawn(build())
    };

    let result = run_with::<A, L, R>(handle.clone(), snapshots, renderer, settings);

    handle.shutdown();
    let _ =
        runtime.block_on(async { tokio::time::timeout(Duration::from_secs(1), task.join()).await });
    result
}

/// Like [`run_with_app`], but starts the actor with caller-owned diagnostics.
///
/// The renderer factory receives the same [`Diagnostics`] handle before the
/// window opens, so a debug overlay can show actor events and a diagnostics
/// shortcut can export errors/config/environment without touching core state.
pub fn run_with_app_diagnostics<A, L, R, F, D>(
    build: F,
    renderer: D,
    settings: Settings,
    diagnostics: Diagnostics,
) -> iced::Result
where
    A: App,
    A::Message: Clone,
    L: Send + 'static,
    R: Renderer<A::View, A::Message, L> + Send + 'static,
    F: FnOnce() -> A,
    D: FnOnce(Diagnostics) -> R,
{
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to start Tokio runtime for the core actor");
    let (handle, snapshots, task) = {
        let _enter = runtime.enter();
        ferriso_core::spawn_with_diagnostics(build(), diagnostics.clone())
    };

    let result = run_with::<A, L, R>(handle.clone(), snapshots, renderer(diagnostics), settings);

    handle.shutdown();
    let _ =
        runtime.block_on(async { tokio::time::timeout(Duration::from_secs(1), task.join()).await });
    result
}

/// Attach an iced window to an existing core actor.
///
/// The window owns only a snapshot and a sender. Closing it does not stop a
/// core that other frontends still use.
pub fn run_with<A, L, R>(
    handle: CoreHandle<A::Message>,
    snapshots: Snapshots<A::View>,
    renderer: R,
    settings: Settings,
) -> iced::Result
where
    A: App,
    A::Message: Clone,
    L: Send + 'static,
    R: Renderer<A::View, A::Message, L> + Send + 'static,
{
    let boot = Mutex::new(Some((snapshots.current(), handle, renderer)));
    let title = settings.title;
    let size = settings.size;
    let application_id = settings.application_id;
    let exit_on_close_request = settings.exit_on_close_request;
    let subscription_snapshots = snapshots.clone();

    iced::application(
        move || {
            let (view, handle, renderer) = boot
                .lock()
                .expect("boot lock poisoned")
                .take()
                .expect("iced booted more than once");
            let fallback_title = title.clone();
            Frontend::new(view, handle, renderer, fallback_title)
        },
        update::<A, L, R>,
        view::<A, L, R>,
    )
    .title(|frontend: &Frontend<A::View, A::Message, L, R>| {
        frontend
            .renderer
            .title(&frontend.view)
            .unwrap_or_else(|| frontend.fallback_title.clone())
    })
    .subscription(move |frontend: &Frontend<A::View, A::Message, L, R>| {
        Subscription::batch([
            subscription(subscription_snapshots.clone()),
            frontend.renderer.subscription(&frontend.view),
        ])
    })
    .theme(|frontend: &Frontend<A::View, A::Message, L, R>| frontend.renderer.theme(&frontend.view))
    .window(window_settings(
        size,
        &application_id,
        exit_on_close_request,
    ))
    .run()
}

/// Window settings shared by every launch path.
fn window_settings(
    size: (f32, f32),
    application_id: &str,
    exit_on_close_request: bool,
) -> iced::window::Settings {
    iced::window::Settings {
        size: iced::Size::new(size.0, size.1),
        exit_on_close_request,
        #[cfg(target_os = "linux")]
        platform_specific: iced::window::settings::PlatformSpecific {
            application_id: application_id.to_owned(),
            ..Default::default()
        },
        #[cfg(not(target_os = "linux"))]
        platform_specific: {
            let _ = application_id;
            Default::default()
        },
        ..Default::default()
    }
}

fn update<A, L, R>(
    frontend: &mut Frontend<A::View, A::Message, L, R>,
    message: Message<A::View, A::Message, L>,
) -> Task<Message<A::View, A::Message, L>>
where
    A: App,
    R: Renderer<A::View, A::Message, L>,
{
    match message {
        Message::User(message) => {
            frontend.renderer.on_message(&message);
            if frontend.handle.send(message) {
                Task::none()
            } else {
                iced::exit()
            }
        }
        Message::Local(local) => frontend.renderer.update(local, &frontend.view),
        Message::Snapshot(view) => frontend.apply_snapshot(view),
        Message::Closed => iced::exit(),
    }
}

fn view<A, L, R>(
    frontend: &Frontend<A::View, A::Message, L, R>,
) -> Element<'_, Message<A::View, A::Message, L>>
where
    A: App,
    R: Renderer<A::View, A::Message, L>,
{
    frontend.renderer.view(&frontend.view)
}

fn subscription<V, M, L: 'static>(snapshots: Snapshots<V>) -> Subscription<Message<V, M, L>>
where
    V: Send + Sync + 'static,
    M: Send + 'static,
{
    Subscription::run_with(snapshots, snapshot_stream::<V>).map(map_snapshot::<V, M, L>)
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

fn map_snapshot<V, M, L: 'static>(event: SnapshotEvent<V>) -> Message<V, M, L> {
    match event {
        SnapshotEvent::Snapshot(view) => Message::Snapshot(view),
        SnapshotEvent::Closed => Message::Closed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_settings_apply_the_requested_size() {
        let settings = window_settings((900.0, 600.0), APPLICATION_ID, true);
        assert_eq!(settings.size.width, 900.0);
        assert_eq!(settings.size.height, 600.0);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn window_settings_expose_a_stable_application_id() {
        let settings = window_settings((900.0, 600.0), APPLICATION_ID, true);
        assert_eq!(settings.platform_specific.application_id, APPLICATION_ID);
    }

    #[test]
    fn snapshot_hooks_run_for_boot_and_later_views() {
        use ferriso_core::Command;

        struct Idle;

        impl App for Idle {
            type Message = u32;
            type View = u32;

            fn update(&mut self, _message: u32) -> Command<u32> {
                Command::none()
            }

            fn view(&self) -> u32 {
                0
            }
        }

        #[derive(Default)]
        struct Hooked {
            seen: Vec<u32>,
        }

        impl Renderer<u32, u32, ()> for Hooked {
            fn view<'a>(&'a self, _view: &'a u32) -> Element<'a, Message<u32, u32, ()>> {
                iced::widget::text("").into()
            }

            fn on_snapshot(&mut self, view: &u32) {
                self.seen.push(*view);
            }
        }

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("test runtime");
        let _enter = runtime.enter();
        let (handle, _snapshots, task) = ferriso_core::spawn(Idle);
        let shutdown = handle.clone();

        let (mut frontend, _boot) = Frontend::<u32, u32, (), Hooked>::new(
            Arc::new(0),
            handle,
            Hooked::default(),
            "test".into(),
        );
        let _later = frontend.apply_snapshot(Arc::new(1));

        assert_eq!(frontend.renderer.seen, vec![0, 1]);

        shutdown.shutdown();
        let _ = runtime
            .block_on(async { tokio::time::timeout(Duration::from_secs(1), task.join()).await });
    }
}
