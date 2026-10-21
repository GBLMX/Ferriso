//! The headless half of Ferriso.
//!
//! `ferriso-core` owns the application state in a Tokio actor, schedules async
//! commands and ticks, accepts frontend-neutral messages, and publishes
//! immutable view snapshots. It depends only on Tokio.
//!
//! Adapters (`ferriso-tui`, `ferriso-gui`) never call [`App::update`]; they send
//! messages and render snapshots with their own native layout.

mod app;
mod command;
mod runtime;

pub use app::App;
pub use command::Command;
pub use ferriso_diagnostics::{Diagnostics, EventKind, FrameworkError, TaskId};
pub use runtime::{
    CoreEvent, CoreHandle, CoreTask, Snapshots, spawn, spawn_with_debug, spawn_with_diagnostics,
};

/// Small prelude for application crates.
pub mod prelude {
    pub use crate::{
        App, Command, CoreHandle, Diagnostics, Snapshots, spawn, spawn_with_diagnostics,
    };
}
