//! Logger interface.
//!
//! The framework never links a logging backend. Applications install any
//! [`Logger`] they like (tracing, `log`, syslog, a test buffer). A logger is
//! filtered twice: by the diagnostics debug switch and by
//! [`Logger::enabled`].

use crate::{DiagnosticError, TaskId};

/// Log severity. Ordering is `Error < Warn < Info < Debug < Trace`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl Level {
    pub fn label(self) -> &'static str {
        match self {
            Self::Error => "ERROR",
            Self::Warn => "WARN",
            Self::Info => "INFO",
            Self::Debug => "DEBUG",
            Self::Trace => "TRACE",
        }
    }
}

/// Structured log record. Context travels with the event, not in the message.
pub struct Record<'a> {
    pub level: Level,
    /// Module or subsystem, e.g. `ferriso-core::actor`.
    pub target: &'a str,
    /// Lifecycle phase, e.g. `init`, `update`, `command`, `config`.
    pub phase: &'a str,
    pub task: TaskId,
    /// Human message. For errors, this is [`Error::message`].
    pub message: &'a str,
    /// The full error, when this record reports a failure.
    pub error: Option<&'a dyn DiagnosticError>,
    pub context: &'a [(String, String)],
}

/// Pluggable logging backend.
///
/// A panic from `enabled` or `log` is contained by [`crate::Diagnostics`] and
/// disables that logger instead of propagating into the actor.
pub trait Logger: Send + Sync + 'static {
    /// Cheap level gate called before a record is built.
    fn enabled(&self, _level: Level) -> bool {
        true
    }

    fn log(&self, record: &Record<'_>);
}

/// Minimal stderr logger suitable for development and headless debugging.
///
/// This is a convenience implementation, not a framework dependency. Swap it
/// for a tracing/`log` adapter with [`crate::Diagnostics::set_logger`].
pub struct StderrLogger {
    minimum: Level,
}

impl StderrLogger {
    pub fn new(minimum: Level) -> Self {
        Self { minimum }
    }
}

impl Logger for StderrLogger {
    fn enabled(&self, level: Level) -> bool {
        level <= self.minimum
    }

    fn log(&self, record: &Record<'_>) {
        let mut line = format!(
            "{} {} phase={} task={} {}",
            record.level.label(),
            record.target,
            record.phase,
            record.task,
            record.message
        );

        if let Some(error) = record.error {
            line = format!(
                "{} {} phase={} task={} {}",
                record.level.label(),
                record.target,
                record.phase,
                record.task,
                crate::error::render(error)
            );
        }

        for (key, value) in record.context {
            line.push_str(&format!(" {key}={value}"));
        }

        eprintln!("{line}");
    }
}
