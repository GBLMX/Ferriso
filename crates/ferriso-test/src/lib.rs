//! Headless test harness for [`ferriso_core::App`] actors.
//!
//! The harness wraps the normal actor entry points so tests can send semantic
//! messages, wait for a view predicate, and shut the actor down without
//! duplicating channel plumbing.
//!
//! ```no_run
//! # async fn example() {
//! use ferriso_core::{App, Command};
//! use ferriso_test::Harness;
//!
//! # struct Counter(u32);
//! # impl App for Counter {
//! #     type Message = u32;
//! #     type View = u32;
//! #     fn update(&mut self, message: u32) -> Command<u32> {
//! #         self.0 += message;
//! #         Command::none()
//! #     }
//! #     fn view(&self) -> u32 { self.0 }
//! # }
//! let mut harness = Harness::new(Counter(0));
//! assert!(harness.send(2));
//! let view = harness.wait_for(|view| *view == 2).await.expect("actor alive");
//! assert_eq!(*view, 2);
//! harness.shutdown().await;
//! # }
//! ```

use std::sync::Arc;

use ferriso_core::{
    App, CoreHandle, CoreTask, Diagnostics, RuntimeOptions, Snapshots, spawn,
    spawn_with_diagnostics, spawn_with_diagnostics_and_options, spawn_with_options,
};

/// Owns one actor and exposes the operations a headless test needs.
///
/// The harness aborts the actor on drop if it has not been shut down. Use
/// [`Harness::shutdown`] when the test should observe the normal shutdown path.
pub struct Harness<A: App> {
    handle: CoreHandle<A::Message>,
    snapshots: Snapshots<A::View>,
    task: Option<CoreTask>,
}

impl<A: App> Harness<A> {
    /// Start the actor with quiet diagnostics and default runtime options.
    ///
    /// Must be called inside a Tokio runtime.
    pub fn new(app: A) -> Self {
        Self::from_parts(spawn(app))
    }

    /// Start the actor with explicit runtime options.
    pub fn with_options(app: A, options: RuntimeOptions) -> Self {
        Self::from_parts(spawn_with_options(app, options))
    }

    /// Start the actor with caller-owned diagnostics.
    pub fn with_diagnostics(app: A, diagnostics: Diagnostics) -> Self {
        Self::from_parts(spawn_with_diagnostics(app, diagnostics))
    }

    /// Start the actor with caller-owned diagnostics and explicit options.
    pub fn with_diagnostics_and_options(
        app: A,
        diagnostics: Diagnostics,
        options: RuntimeOptions,
    ) -> Self {
        Self::from_parts(spawn_with_diagnostics_and_options(
            app,
            diagnostics,
            options,
        ))
    }

    fn from_parts(
        (handle, snapshots, task): (CoreHandle<A::Message>, Snapshots<A::View>, CoreTask),
    ) -> Self {
        Self {
            handle,
            snapshots,
            task: Some(task),
        }
    }

    /// The underlying actor handle, for direct control operations.
    pub fn handle(&self) -> &CoreHandle<A::Message> {
        &self.handle
    }

    /// Send one semantic message. Returns `false` once the actor has stopped.
    pub fn send(&self, message: A::Message) -> bool {
        self.handle.send(message)
    }

    /// Latest published view.
    pub fn view(&self) -> Arc<A::View> {
        self.snapshots.current()
    }

    /// A clone of the snapshot stream, for observation after the harness is
    /// dropped or moved into another task.
    pub fn snapshots(&self) -> Snapshots<A::View> {
        self.snapshots.clone()
    }

    /// Whether the actor has stopped publishing snapshots.
    pub fn closed(&self) -> bool {
        self.snapshots.closed()
    }

    /// Wait until a view satisfies `predicate`.
    ///
    /// Checks the latest view first, then waits for changes. Returns `None` if
    /// the actor exits before the predicate matches. The test should wrap this
    /// in a timeout when a missing transition would otherwise hang.
    pub async fn wait_for<F>(&mut self, mut predicate: F) -> Option<Arc<A::View>>
    where
        F: FnMut(&A::View) -> bool,
    {
        let current = self.snapshots.current();
        if predicate(&current) {
            return Some(current);
        }

        loop {
            match self.snapshots.changed().await {
                Ok(view) => {
                    if predicate(&view) {
                        return Some(view);
                    }
                }
                Err(_) => return None,
            }
        }
    }

    /// Ask the actor to stop and wait for its task to finish.
    ///
    /// This runs the normal shutdown path, including `App::shutdown`.
    pub async fn shutdown(mut self) {
        self.handle.shutdown();
        if let Some(task) = self.task.take() {
            task.join().await;
        }
    }
}

impl<A: App> Drop for Harness<A> {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}
