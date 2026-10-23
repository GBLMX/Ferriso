//! `tracing` backend for [`ferriso_diagnostics::Logger`].
//!
//! Ferriso diagnostics keeps the logging contract backend-neutral. This crate
//! is the opt-in bridge for applications that already use `tracing`; install
//! tracing filters, formatters, and OpenTelemetry layers on the subscriber as
//! usual.
//!
//! ```no_run
//! use std::sync::Arc;
//! use ferriso_diagnostics::Diagnostics;
//! use ferriso_tracing::TracingLogger;
//!
//! let diagnostics = Diagnostics::new()
//!     .with_debug(true)
//!     .with_logger(Arc::new(TracingLogger::new()));
//! ```

use ferriso_diagnostics::{Level, Logger, Record};

/// A [`Logger`] that emits one `tracing` event per diagnostics record.
///
/// The event keeps the diagnostics target, phase, task id, message, and error
/// as structured fields. Filtering is left to the installed tracing subscriber.
#[derive(Clone, Copy, Debug, Default)]
pub struct TracingLogger;

impl TracingLogger {
    /// Create a logger. It has no state; clone it freely.
    pub const fn new() -> Self {
        Self
    }
}

impl Logger for TracingLogger {
    fn log(&self, record: &Record<'_>) {
        // `tracing` callsites require a constant target and level, so keep one
        // stable callsite target and carry the diagnostics target as a field.
        macro_rules! emit {
            ($level:ident) => {
                tracing::$level!(
                    target: "ferriso",
                    diagnostics_target = record.target,
                    phase = record.phase,
                    task = record.task.get(),
                    message = record.message,
                    error = ?record.error,
                )
            };
        }

        match record.level {
            Level::Error => emit!(error),
            Level::Warn => emit!(warn),
            Level::Info => emit!(info),
            Level::Debug => emit!(debug),
            Level::Trace => emit!(trace),
        }
    }
}
