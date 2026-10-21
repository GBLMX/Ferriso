//! Minimal diagnostics example: plug a logger, wrap an error, export a report.
//!
//! ```bash
//! cargo run -p ferriso-diagnostics --example doctor
//! ```

use std::{io, sync::Arc};

use ferriso_diagnostics::{
    Diagnostics, ErrorSnapshot, Level, Logger, Record, ReportMeta, TaskId, safe_environment,
    state_dir,
};

struct ConsoleLogger;

impl Logger for ConsoleLogger {
    fn enabled(&self, level: Level) -> bool {
        level <= Level::Debug
    }

    fn log(&self, record: &Record<'_>) {
        println!(
            "{} {} phase={} task={} {}",
            record.level.label(),
            record.target,
            record.phase,
            record.task,
            record.message
        );
        if let Some(error) = record.error {
            println!("  {}", ferriso_diagnostics::render(error));
        }
    }
}

#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[error("configuration file {path} is not readable")]
#[diagnostic(
    code(example::config::read),
    help("Fix permissions on {path}, or remove it so built-in defaults apply.")
)]
struct ConfigError {
    path: String,
    #[source]
    source: io::Error,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let diagnostics = Diagnostics::new()
        .with_debug(true)
        .with_logger(Arc::new(ConsoleLogger));

    diagnostics.log(
        Level::Info,
        "doctor",
        "startup",
        TaskId::NONE,
        "diagnostics example started",
    );

    let error = ConfigError {
        path: "~/.config/example/config.toml".into(),
        source: io::Error::new(io::ErrorKind::PermissionDenied, "permission denied"),
    };
    diagnostics.error("config", TaskId::NONE, &error);

    let meta = ReportMeta {
        application: "doctor",
        version: env!("CARGO_PKG_VERSION"),
        config: "[example]\nbackend = \"missing\"",
        environment: &safe_environment(),
    };

    let directory = state_dir("ferriso-doctor").unwrap_or_else(std::env::temp_dir);
    let path = directory.join("diagnostics.txt");
    let written = diagnostics.export(&path, &meta)?;

    println!("\nexported: {}", written.display());
    println!("recent errors: {}", diagnostics.recent_errors().len());
    print_error(&diagnostics.recent_errors()[0]);
    Ok(())
}

fn print_error(error: &ErrorSnapshot) {
    println!(
        "  [{}] {} phase={} task={}",
        error.code.as_deref().unwrap_or("ferriso::unknown"),
        error.message,
        error.phase,
        error.task
    );
    if let Some(help) = &error.help {
        println!("  help: {help}");
    }
    for cause in &error.causes {
        println!("  caused by: {cause}");
    }
}
