//! The headless half of Ferriso.
//!
//! `ferriso-core` owns the application state in a Tokio actor, schedules async
//! commands, reconciles keyed subscription streams, provides cooperative
//! cancellation, accepts frontend-neutral messages, and publishes immutable
//! view snapshots. Its dependencies are Tokio, futures, and
//! `ferriso-diagnostics`.
//!
//! Adapters (`ferriso-tui`, `ferriso-gui`) never call [`App::update`]; they send
//! messages and render snapshots with their own native layout.

mod app;
mod cancellation;
mod command;
mod runtime;
mod subscription;

pub use app::App;
pub use cancellation::CancellationToken;
pub use command::Command;
pub use ferriso_diagnostics::{Diagnostics, EventKind, FrameworkError, TaskId};
pub use runtime::{
    CoreContext, CoreEvent, CoreHandle, CoreTask, RuntimeOptions, Sidecar, Sidecars, Snapshots,
    spawn, spawn_with_debug, spawn_with_diagnostics, spawn_with_diagnostics_and_options,
    spawn_with_options,
};
pub use subscription::Subscription;

/// Small prelude for application crates.
pub mod prelude {
    pub use crate::{
        App, CancellationToken, Command, CoreContext, CoreHandle, Diagnostics, RuntimeOptions,
        Sidecar, Sidecars, Snapshots, Subscription, spawn, spawn_with_diagnostics,
        spawn_with_options,
    };
}
