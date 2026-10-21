//! Ferriso's diagnostics contract.
//!
//! Four pieces, one mechanism:
//!
//! - [`DiagnosticError`]: any `thiserror` + `miette` error. Codes, help, and
//!   sources stay structured; causes are never flattened away.
//! - [`Logger`]: a trait, not a backend. Applications plug in tracing, `log`,
//!   stderr, or a test buffer. Production installs nothing and stays quiet.
//! - [`Diagnostics`]: runtime debug switch, bounded event/error rings, task ids,
//!   and a one-click [`Diagnostics::export`] report.
//! - [`safe_environment`]: report facts without dumping secrets.
//!
//! ```no_run
//! use std::sync::Arc;
//! use ferriso_diagnostics::{
//!     Diagnostics, FrameworkError, Level, Logger, Record, ReportMeta,
//! };
//!
//! struct MyLogger;
//! impl Logger for MyLogger {
//!     fn log(&self, record: &Record<'_>) {
//!         println!("{} {}: {}", record.level.label(), record.target, record.message);
//!     }
//! }
//!
//! let diagnostics = Diagnostics::new()
//!     .with_debug(true)
//!     .with_logger(Arc::new(MyLogger));
//!
//! let error = FrameworkError::ActorPanic {
//!     phase: "update",
//!     message: "example panic".into(),
//!     task: 1,
//! };
//! diagnostics.error("update", ferriso_diagnostics::TaskId::NONE, &error);
//!
//! let meta = ReportMeta {
//!     application: "example",
//!     version: env!("CARGO_PKG_VERSION"),
//!     config: "[backend]\nurl = \"\"",
//!     environment: &ferriso_diagnostics::safe_environment(),
//! };
//! diagnostics
//!     .export(std::path::Path::new("/tmp/example-diagnostics.txt"), &meta)
//!     .expect("write report");
//!
//! let _ = Level::Info;
//! ```

use std::fmt::{self, Display};

pub mod diagnostics;
pub mod error;
pub mod log;

pub use diagnostics::{
    Diagnostics, ErrorSnapshot, Event, EventKind, ReportMeta, safe_environment, state_dir,
    unix_seconds,
};
pub use error::{DiagnosticError, FrameworkError, cause_chain, code, render, severity};

/// Framework result alias. Applications may substitute their own error type.
pub type Result<T, E = miette::Report> = std::result::Result<T, E>;
pub use log::{Level, Logger, Record, StderrLogger};

/// Identity assigned by [`Diagnostics::next_task`] to an actor, request, or job.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TaskId(u64);

impl TaskId {
    /// No task, for process-level logging outside an actor.
    pub const NONE: Self = Self(0);

    pub(crate) fn new(value: u64) -> Self {
        Self(value)
    }

    pub fn get(self) -> u64 {
        self.0
    }
}

impl Display for TaskId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "#{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_ids_display_with_a_prefix() {
        assert_eq!(TaskId::NONE.to_string(), "#0");
        assert_eq!(TaskId::new(7).to_string(), "#7");
    }
}
