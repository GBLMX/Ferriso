//! Framework error contract built on `thiserror` and `miette`.
//!
//! Concrete failures are typed enums that derive [`thiserror::Error`] (source
//! preservation) and [`miette::Diagnostic`] (stable code, help, severity).
//! Logging and diagnostics accept any [`DiagnosticError`], so applications can
//! plug their own error types without adopting a framework enum.

use miette::{Diagnostic, Severity};
use std::error::Error as StdError;

/// Any error that carries `miette` diagnostic metadata and a preserved cause.
pub trait DiagnosticError: StdError + Diagnostic + Send + Sync + 'static {}

impl<T> DiagnosticError for T where T: StdError + Diagnostic + Send + Sync + 'static {}

/// Errors raised by the Ferriso runtime itself at a user-code boundary.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum FrameworkError {
    #[error("user code panicked during `{phase}`: {message}")]
    #[diagnostic(
        code(ferriso::actor::panic),
        help(
            "The actor stopped at this boundary to avoid running on inconsistent state. Export diagnostics (recent errors, config, environment) and report the panic with that bundle."
        )
    )]
    ActorPanic {
        phase: &'static str,
        message: String,
        task: u64,
    },

    #[error("a command future panicked during `{phase}`: {message}")]
    #[diagnostic(
        code(ferriso::command::panic),
        help(
            "The actor is still running, but the command's result was dropped. Find the panicking future and return an error message instead."
        )
    )]
    CommandPanic {
        phase: &'static str,
        message: String,
        task: u64,
    },

    #[error("could not export the diagnostics report to {path}")]
    #[diagnostic(
        code(ferriso::diagnostics::export),
        help(
            "Choose a writable path, check free space, or set XDG_STATE_HOME to a writable directory."
        )
    )]
    Export {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

/// One-line rendering used by loggers: code, message, help, then causes.
pub fn render(error: &dyn DiagnosticError) -> String {
    let code = error
        .code()
        .map(|code| code.to_string())
        .unwrap_or_else(|| "ferriso::unknown".into());
    let mut rendered = format!("[{code}] {error}");

    if let Some(help) = error.help() {
        rendered.push_str(" — hint: ");
        rendered.push_str(&help.to_string());
    }

    for cause in cause_chain(error) {
        rendered.push_str(" — caused by: ");
        rendered.push_str(&cause);
    }

    rendered
}

/// Source chain messages, outermost first.
pub fn cause_chain(error: &dyn DiagnosticError) -> Vec<String> {
    let mut causes = Vec::new();
    let mut source = error.source();

    while let Some(cause) = source {
        causes.push(cause.to_string());
        source = cause.source();
    }

    causes
}

/// Severity defaults to [`Severity::Error`] when the diagnostic omits it.
pub fn severity(error: &dyn DiagnosticError) -> Severity {
    error.severity().unwrap_or(Severity::Error)
}

/// Stable code string for reports. `None` means the error did not declare one.
pub fn code(error: &dyn DiagnosticError) -> Option<String> {
    error.code().map(|code| code.to_string())
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;

    #[derive(Debug, thiserror::Error, miette::Diagnostic)]
    #[error("could not read {path}")]
    #[diagnostic(code(test::io), help("Check the path and permissions."))]
    struct Fixture {
        path: String,
        #[source]
        source: io::Error,
    }

    #[test]
    fn renders_code_help_and_preserved_cause() {
        let error = Fixture {
            path: "config.toml".into(),
            source: io::Error::new(io::ErrorKind::NotFound, "no such file"),
        };
        let rendered = render(&error);

        assert!(rendered.contains("[test::io]"));
        assert!(rendered.contains("Check the path"));
        assert!(rendered.contains("caused by: no such file"));
        assert_eq!(severity(&error), Severity::Error);
        assert_eq!(code(&error).as_deref(), Some("test::io"));
        assert_eq!(
            error.source().map(ToString::to_string),
            Some("no such file".into())
        );
    }
}
