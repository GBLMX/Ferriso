//! Native platform services shared by Ferriso applications.
//!
//! Services expose narrow traits so applications can inject deterministic
//! test doubles and keep the system implementations replaceable. The opener
//! launches the desktop's default application; the clipboard reads and writes
//! UTF-8 text.
//!
//! Native backends are optional so an app only links what it uses:
//!
//! - `clipboard`: `arboard` clipboard access;
//! - `dialogs`: `rfd` file and folder pickers;
//! - `notifications`: `notify-rust` desktop notifications;
//! - `watch`: `notify` file watching.
//!
//! All four are enabled by default for compatibility. Use
//! `default-features = false` to select only the services an application needs;
//! the opener is always available because it only uses the standard library.

#[cfg(feature = "clipboard")]
mod clipboard;
#[cfg(feature = "dialogs")]
mod dialog;
#[cfg(feature = "notifications")]
mod notifications;
#[cfg(feature = "watch")]
mod watch;

#[cfg(feature = "clipboard")]
pub use clipboard::{Clipboard, ClipboardError, MemoryClipboard, SystemClipboard};
#[cfg(feature = "dialogs")]
pub use dialog::{FileDialog, MemoryFileDialog, SystemFileDialog};
#[cfg(feature = "notifications")]
pub use notifications::{
    MemoryNotifier, Notification, NotificationError, Notifier, SystemNotifier,
};
#[cfg(feature = "watch")]
pub use watch::{FileWatcher, MemoryFileWatcher, SystemFileWatcher, Watch, WatchError, WatchEvent};

use std::{
    error::Error as StdError,
    fmt, io,
    path::{Path, PathBuf},
    process::Command,
};

/// Open a filesystem path with the platform's default application.
pub trait Opener: Send + Sync + 'static {
    /// Launch the default application for `target`.
    fn open(&self, target: &Path) -> Result<(), OpenError>;
}

/// The host desktop's default opener (`xdg-open`, `open`, or `start`).
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemOpener;

impl SystemOpener {
    /// Create a stateless system opener.
    pub const fn new() -> Self {
        Self
    }
}

impl Opener for SystemOpener {
    fn open(&self, target: &Path) -> Result<(), OpenError> {
        #[cfg(target_os = "linux")]
        {
            let mut command = Command::new("xdg-open");
            command.arg(target);
            command.spawn().map(|_| ()).map_err(|source| OpenError {
                target: target.to_path_buf(),
                program: "xdg-open",
                source,
            })
        }

        #[cfg(target_os = "macos")]
        {
            let mut command = Command::new("open");
            command.arg(target);
            command.spawn().map(|_| ()).map_err(|source| OpenError {
                target: target.to_path_buf(),
                program: "open",
                source,
            })
        }

        #[cfg(target_os = "windows")]
        {
            let mut command = Command::new("cmd");
            command.args(["/C", "start", ""]).arg(target);
            command.spawn().map(|_| ()).map_err(|source| OpenError {
                target: target.to_path_buf(),
                program: "cmd",
                source,
            })
        }

        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            let _ = target;
            Err(OpenError {
                target: target.to_path_buf(),
                program: "platform",
                source: io::Error::new(
                    io::ErrorKind::Unsupported,
                    "no system opener is configured for this platform",
                ),
            })
        }
    }
}

/// Failure to launch the platform opener.
#[derive(Debug)]
pub struct OpenError {
    target: PathBuf,
    program: &'static str,
    source: io::Error,
}

impl OpenError {
    /// The path that could not be opened.
    pub fn target(&self) -> &Path {
        &self.target
    }

    /// The platform program that failed to launch.
    pub fn program(&self) -> &'static str {
        self.program
    }

    /// The underlying process error.
    pub fn io_error(&self) -> &io::Error {
        &self.source
    }
}

impl fmt::Display for OpenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "could not open {} with {}: {}",
            self.target.display(),
            self.program,
            self.source
        )
    }
}

impl StdError for OpenError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(&self.source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_reports_target_program_and_source() {
        let error = OpenError {
            target: PathBuf::from("/tmp/example"),
            program: "test-opener",
            source: io::Error::new(io::ErrorKind::NotFound, "missing"),
        };

        assert_eq!(error.target(), Path::new("/tmp/example"));
        assert_eq!(error.program(), "test-opener");
        assert_eq!(error.io_error().kind(), io::ErrorKind::NotFound);
        assert!(error.to_string().contains("/tmp/example"));
        assert!(error.source().is_some());
    }
}
