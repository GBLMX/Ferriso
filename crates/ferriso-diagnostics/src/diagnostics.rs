//! Debug switch, bounded event/error rings, and one-click report export.

use std::{
    collections::VecDeque,
    env, fmt, fs,
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use miette::Severity;

use crate::{
    DiagnosticError, FrameworkError, TaskId,
    error::{cause_chain, code, render, severity},
    log::{Level, Logger, Record},
};

/// Structured actor event kinds used by debug overlays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    Lifecycle,
    Message,
    Tick,
    Command,
    CommandResult,
    Subscription,
    Snapshot,
    Error,
    Custom,
}

impl EventKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Lifecycle => "lifecycle",
            Self::Message => "message",
            Self::Tick => "tick",
            Self::Command => "command",
            Self::CommandResult => "command-result",
            Self::Subscription => "subscription",
            Self::Snapshot => "snapshot",
            Self::Error => "error",
            Self::Custom => "event",
        }
    }
}

/// One retained debug event.
#[derive(Clone, Debug)]
pub struct Event {
    pub at: Duration,
    pub task: TaskId,
    pub kind: EventKind,
    pub phase: &'static str,
    pub detail: String,
}

/// One retained error, enough to diagnose without the original error object.
#[derive(Clone, Debug)]
pub struct ErrorSnapshot {
    pub at: Duration,
    pub task: TaskId,
    pub phase: &'static str,
    pub severity: Severity,
    pub code: Option<String>,
    pub message: String,
    pub help: Option<String>,
    pub causes: Vec<String>,
}

/// Caller-supplied facts for a diagnostics report.
pub struct ReportMeta<'a> {
    pub application: &'a str,
    pub version: &'a str,
    /// Human-readable, secret-free configuration summary.
    pub config: &'a str,
    /// Safe environment facts, typically from [`safe_environment`].
    pub environment: &'a [(String, String)],
}

struct Inner {
    started: Instant,
    debug: AtomicBool,
    logger: RwLock<Option<Arc<dyn Logger>>>,
    events: Mutex<VecDeque<Event>>,
    errors: Mutex<VecDeque<ErrorSnapshot>>,
    next_task: AtomicU64,
    event_count: AtomicU64,
    error_count: AtomicU64,
    capacity: usize,
}

/// Cloneable diagnostics handle shared by the actor and frontends.
///
/// Production default: debug off, no logger, no output. Errors are still kept
/// in a small ring so an explicit report can include them.
#[derive(Clone)]
pub struct Diagnostics {
    inner: Arc<Inner>,
}

impl Diagnostics {
    pub fn new() -> Self {
        Self::with_capacity(512)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: Arc::new(Inner {
                started: Instant::now(),
                debug: AtomicBool::new(false),
                logger: RwLock::new(None),
                events: Mutex::new(VecDeque::new()),
                errors: Mutex::new(VecDeque::new()),
                next_task: AtomicU64::new(1),
                event_count: AtomicU64::new(0),
                error_count: AtomicU64::new(0),
                capacity: capacity.max(1),
            }),
        }
    }

    #[must_use]
    pub fn with_debug(self, debug: bool) -> Self {
        self.set_debug(debug);
        self
    }

    #[must_use]
    pub fn with_logger(self, logger: Arc<dyn Logger>) -> Self {
        self.set_logger(logger);
        self
    }

    /// Turn verbose events and `Info`/`Debug`/`Trace` logs on or off at runtime.
    pub fn set_debug(&self, debug: bool) {
        self.inner.debug.store(debug, Ordering::Relaxed);
    }

    pub fn debug_enabled(&self) -> bool {
        self.inner.debug.load(Ordering::Relaxed)
    }

    pub fn set_logger(&self, logger: Arc<dyn Logger>) {
        if let Ok(mut slot) = self.inner.logger.write() {
            *slot = Some(logger);
        }
    }

    pub fn clear_logger(&self) {
        if let Ok(mut slot) = self.inner.logger.write() {
            *slot = None;
        }
    }

    pub fn has_logger(&self) -> bool {
        self.inner
            .logger
            .read()
            .map(|slot| slot.is_some())
            .unwrap_or(false)
    }

    /// Allocate an id for one actor/request/task.
    pub fn next_task(&self) -> TaskId {
        TaskId::new(self.inner.next_task.fetch_add(1, Ordering::Relaxed))
    }

    /// Record a debug event and forward it to the logger at `Debug`.
    ///
    /// Does nothing unless debug mode is on.
    pub fn event(
        &self,
        kind: EventKind,
        target: &str,
        phase: &'static str,
        task: TaskId,
        detail: impl AsRef<str>,
    ) {
        if !self.debug_enabled() {
            return;
        }

        let detail = detail.as_ref();
        if let Ok(mut events) = self.inner.events.lock() {
            if events.len() >= self.inner.capacity {
                events.pop_front();
            }
            events.push_back(Event {
                at: self.inner.started.elapsed(),
                task,
                kind,
                phase,
                detail: detail.to_owned(),
            });
        }
        self.inner.event_count.fetch_add(1, Ordering::Relaxed);
        self.log(Level::Debug, target, phase, task, detail);
    }

    /// Emit one structured log record.
    ///
    /// `Warn`/`Error` are always eligible; `Info` and below require debug mode.
    /// Nothing is written without an installed logger.
    pub fn log(&self, level: Level, target: &str, phase: &str, task: TaskId, message: &str) {
        self.forward(&Record {
            level,
            target,
            phase,
            task,
            message,
            error: None,
            context: &[],
        });
    }

    /// Record an error in the diagnostics ring and forward it to the logger.
    ///
    /// Errors are retained even in production so [`Diagnostics::export`] can
    /// include the most recent ones.
    pub fn error(&self, phase: &'static str, task: TaskId, error: &dyn DiagnosticError) {
        let message = error.to_string();
        let snapshot = ErrorSnapshot {
            at: self.inner.started.elapsed(),
            task,
            phase,
            severity: severity(error),
            code: code(error),
            message: message.clone(),
            help: error.help().map(|help| help.to_string()),
            causes: cause_chain(error),
        };

        if let Ok(mut errors) = self.inner.errors.lock() {
            if errors.len() >= 64 {
                errors.pop_front();
            }
            errors.push_back(snapshot);
        }
        self.inner.error_count.fetch_add(1, Ordering::Relaxed);

        if self.debug_enabled()
            && let Ok(mut events) = self.inner.events.lock()
        {
            if events.len() >= self.inner.capacity {
                events.pop_front();
            }
            events.push_back(Event {
                at: self.inner.started.elapsed(),
                task,
                kind: EventKind::Error,
                phase,
                detail: render(error),
            });
            self.inner.event_count.fetch_add(1, Ordering::Relaxed);
        }

        self.forward(&Record {
            level: Level::Error,
            target: "ferriso",
            phase,
            task,
            message: &message,
            error: Some(error),
            context: &[],
        });
    }

    pub fn events(&self) -> Vec<Event> {
        self.inner
            .events
            .lock()
            .map(|events| events.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn recent_errors(&self) -> Vec<ErrorSnapshot> {
        self.inner
            .errors
            .lock()
            .map(|errors| errors.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn clear_events(&self) {
        if let Ok(mut events) = self.inner.events.lock() {
            events.clear();
        }
    }

    pub fn clear_errors(&self) {
        if let Ok(mut errors) = self.inner.errors.lock() {
            errors.clear();
        }
    }

    pub fn total_events(&self) -> u64 {
        self.inner.event_count.load(Ordering::Relaxed)
    }

    pub fn total_errors(&self) -> u64 {
        self.inner.error_count.load(Ordering::Relaxed)
    }

    /// Human-readable report with environment, config, errors, and events.
    pub fn report(&self, meta: &ReportMeta<'_>) -> String {
        let mut report = String::new();
        report.push_str("Ferriso diagnostics report\n");
        report.push_str(&format!("generated_unix: {}\n", unix_seconds()));
        report.push_str(&format!("application: {}\n", meta.application));
        report.push_str(&format!("version: {}\n", meta.version));
        report.push_str(&format!("debug: {}\n", self.debug_enabled()));
        report.push_str(&format!(
            "logger: {}\n",
            if self.has_logger() {
                "installed"
            } else {
                "none"
            }
        ));
        report.push_str(&format!(
            "uptime_ms: {:.1}\n",
            self.uptime().as_secs_f64() * 1000.0
        ));

        report.push_str("\nenvironment:\n");
        if meta.environment.is_empty() {
            report.push_str("  (none supplied)\n");
        }
        for (key, value) in meta.environment {
            report.push_str(&format!("  {key}={value}\n"));
        }

        report.push_str("\nconfig:\n");
        for line in meta.config.lines() {
            report.push_str(&format!("  {line}\n"));
        }

        let errors = self.recent_errors();
        report.push_str(&format!(
            "\nrecent errors: {} retained, {} total\n",
            errors.len(),
            self.total_errors()
        ));
        if errors.is_empty() {
            report.push_str("  (none)\n");
        }
        for error in errors {
            report.push_str(&format!(
                "  [{}] {:?} phase={} task={} at={:.1}ms\n",
                error.code.as_deref().unwrap_or("ferriso::unknown"),
                error.severity,
                error.phase,
                error.task,
                error.at.as_secs_f64() * 1000.0
            ));
            report.push_str(&format!("    {}\n", error.message));
            if let Some(help) = &error.help {
                report.push_str(&format!("    hint: {help}\n"));
            }
            for cause in &error.causes {
                report.push_str(&format!("    caused by: {cause}\n"));
            }
        }

        let events = self.events();
        report.push_str(&format!(
            "\nrecent events: {} retained, {} total\n",
            events.len(),
            self.total_events()
        ));
        if events.is_empty() {
            report.push_str("  (debug events disabled or none recorded)\n");
        }
        for event in events.iter().rev().take(40).rev() {
            report.push_str(&format!(
                "  {:>9.1}ms {:<15} {:<11} task={} {}\n",
                event.at.as_secs_f64() * 1000.0,
                event.kind.label(),
                event.phase,
                event.task,
                event.detail
            ));
        }

        report
    }

    /// Write a report to `path`, creating parent directories.
    pub fn export(
        &self,
        path: &Path,
        meta: &ReportMeta<'_>,
    ) -> std::result::Result<PathBuf, FrameworkError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| FrameworkError::Export {
                path: parent.display().to_string(),
                source,
            })?;
        }

        fs::write(path, self.report(meta)).map_err(|source| FrameworkError::Export {
            path: path.display().to_string(),
            source,
        })?;

        Ok(path.to_path_buf())
    }

    fn uptime(&self) -> Duration {
        self.inner.started.elapsed()
    }

    fn level_enabled(&self, level: Level) -> bool {
        level <= Level::Warn || self.debug_enabled()
    }

    fn forward(&self, record: &Record<'_>) {
        if !self.level_enabled(record.level) {
            return;
        }

        let logger = self.inner.logger.read().ok().and_then(|slot| slot.clone());

        let Some(logger) = logger else {
            return;
        };
        match catch_unwind(AssertUnwindSafe(|| logger.enabled(record.level))) {
            Ok(true) => {}
            Ok(false) => return,
            Err(_) => {
                self.clear_logger();
                return;
            }
        }

        // A logger is an extension point and may be third-party code. It must
        // not take down the actor while the actor is already handling an error.
        if catch_unwind(AssertUnwindSafe(|| logger.log(record))).is_err() {
            self.clear_logger();
        }
    }
}

impl Default for Diagnostics {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for Diagnostics {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Diagnostics")
            .field("debug", &self.debug_enabled())
            .field("logger", &self.has_logger())
            .field("events", &self.total_events())
            .field("errors", &self.total_errors())
            .finish()
    }
}

/// Safe environment facts for reports. Never dumps the whole environment, so
/// tokens and secrets cannot leak into an exported bundle.
pub fn safe_environment() -> Vec<(String, String)> {
    let mut facts = vec![
        ("os".into(), env::consts::OS.into()),
        ("arch".into(), env::consts::ARCH.into()),
        ("family".into(), env::consts::FAMILY.into()),
    ];

    for key in [
        "XDG_SESSION_TYPE",
        "XDG_CURRENT_DESKTOP",
        "WAYLAND_DISPLAY",
        "DISPLAY",
        "TERM",
        "RUST_LOG",
        "RUST_BACKTRACE",
    ] {
        if let Ok(value) = env::var(key)
            && !value.is_empty()
        {
            facts.push((key.to_owned(), value));
        }
    }

    facts
}

/// Per-user state directory: `$XDG_STATE_HOME/<app>`, else
/// `~/.local/state/<app>` (Windows: `%LOCALAPPDATA%\<app>`).
pub fn state_dir(app: &str) -> Option<PathBuf> {
    let base = if cfg!(windows) {
        env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else if let Some(path) = env::var_os("XDG_STATE_HOME").filter(|value| !value.is_empty()) {
        Some(PathBuf::from(path))
    } else {
        env::var_os("HOME")
            .filter(|value| !value.is_empty())
            .map(|home| PathBuf::from(home).join(".local/state"))
    };

    base.map(|base| base.join(app))
}

/// Seconds since the Unix epoch, for report filenames and timestamps.
pub fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::{
        io,
        sync::{Arc, Mutex},
    };

    use super::*;
    use crate::TaskId;

    #[derive(Default)]
    struct Buffer {
        records: Arc<Mutex<Vec<String>>>,
    }

    impl Logger for Buffer {
        fn log(&self, record: &Record<'_>) {
            self.records.lock().unwrap().push(format!(
                "{} {} {} {}",
                record.level.label(),
                record.phase,
                record.task,
                record.message
            ));
        }
    }

    #[derive(Debug, thiserror::Error, miette::Diagnostic)]
    #[error("bad key in config")]
    #[diagnostic(code(test::config), help("Remove the key."))]
    struct TestError {
        #[source]
        source: io::Error,
    }

    fn test_error() -> TestError {
        TestError {
            source: io::Error::new(io::ErrorKind::InvalidData, "unknown field"),
        }
    }

    #[test]
    fn quiet_by_default_and_errors_are_retained() {
        let diagnostics = Diagnostics::new();
        diagnostics.event(EventKind::Message, "core", "update", TaskId::NONE, "hello");
        diagnostics.error("update", TaskId::NONE, &test_error());

        assert!(diagnostics.events().is_empty());
        assert_eq!(diagnostics.recent_errors().len(), 1);
        assert_eq!(diagnostics.total_events(), 0);
    }

    #[test]
    fn debug_events_and_logs_are_bounded() {
        let buffer = Buffer::default();
        let records = buffer.records.clone();
        let diagnostics = Diagnostics::with_capacity(2)
            .with_debug(true)
            .with_logger(Arc::new(buffer));

        for index in 0..3 {
            diagnostics.event(
                EventKind::Message,
                "core",
                "update",
                TaskId::NONE,
                format!("message {index}"),
            );
        }

        let events = diagnostics.events();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].detail, "message 1");
        assert_eq!(diagnostics.total_events(), 3);
        assert_eq!(records.lock().unwrap().len(), 3);
    }

    #[test]
    fn info_is_silent_without_debug_but_warn_is_not() {
        let buffer = Buffer::default();
        let records = buffer.records.clone();
        let diagnostics = Diagnostics::new().with_logger(Arc::new(buffer));

        diagnostics.log(Level::Info, "core", "start", TaskId::NONE, "info");
        diagnostics.log(Level::Warn, "core", "start", TaskId::NONE, "warn");
        diagnostics.error("start", TaskId::NONE, &test_error());

        let records = records.lock().unwrap();
        assert_eq!(records.len(), 2);
        assert!(records[0].contains("WARN"));
        assert!(records[1].contains("ERROR"));
    }

    struct PanickingLogger;

    impl Logger for PanickingLogger {
        fn log(&self, _record: &Record<'_>) {
            panic!("logger panic");
        }
    }

    struct PanickingGate;

    impl Logger for PanickingGate {
        fn enabled(&self, _level: Level) -> bool {
            panic!("logger gate panic");
        }

        fn log(&self, _record: &Record<'_>) {}
    }

    #[test]
    fn a_panicking_logger_is_disabled_instead_of_propagating() {
        let diagnostics = Diagnostics::new().with_logger(Arc::new(PanickingLogger));
        diagnostics.log(Level::Warn, "test", "log", TaskId::NONE, "warn");
        assert!(!diagnostics.has_logger());

        let diagnostics = Diagnostics::new().with_logger(Arc::new(PanickingGate));
        diagnostics.log(Level::Warn, "test", "log", TaskId::NONE, "warn");
        assert!(!diagnostics.has_logger());
    }
    #[test]
    fn report_carries_config_help_and_cause() {
        let diagnostics = Diagnostics::new().with_debug(true);
        diagnostics.error("config", TaskId::NONE, &test_error());
        diagnostics.event(
            EventKind::Lifecycle,
            "core",
            "init",
            TaskId::NONE,
            "started",
        );

        let meta = ReportMeta {
            application: "fixture",
            version: env!("CARGO_PKG_VERSION"),
            config: "[debug]\nenabled = true",
            environment: &[("os".into(), "linux".into())],
        };
        let report = diagnostics.report(&meta);

        assert!(report.contains("application: fixture"));
        assert!(report.contains("[test::config]"));
        assert!(report.contains("Remove the key."));
        assert!(report.contains("caused by: unknown field"));
        assert!(report.contains("enabled = true"));
        assert!(report.contains("lifecycle"));
    }

    #[test]
    fn export_creates_parent_directories() {
        let path = env::temp_dir().join(format!(
            "ferriso-diagnostics-test-{}/nested/report.txt",
            unix_seconds()
        ));
        let _ = fs::remove_dir_all(path.parent().unwrap().parent().unwrap());

        let diagnostics = Diagnostics::new();
        let meta = ReportMeta {
            application: "fixture",
            version: env!("CARGO_PKG_VERSION"),
            config: "",
            environment: &[],
        };
        let written = diagnostics.export(&path, &meta).expect("export");
        assert_eq!(written, path);
        assert!(
            fs::read_to_string(&path)
                .unwrap()
                .contains("Ferriso diagnostics")
        );
        let _ = fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
    }
}
