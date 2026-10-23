//! Clipboard access with a real system backend and an in-memory test double.

use std::{error::Error as StdError, fmt, sync::Mutex};

use arboard::Clipboard as Arboard;

/// Read and write UTF-8 text on the platform clipboard.
pub trait Clipboard: Send + Sync + 'static {
    /// Read text, or `None` when the clipboard is empty or holds another format.
    fn get_text(&self) -> Result<Option<String>, ClipboardError>;

    /// Replace the clipboard contents with UTF-8 text.
    fn set_text(&self, text: &str) -> Result<(), ClipboardError>;
}

/// The host clipboard through [`arboard`].
///
/// Keep the instance alive for as long as the application writes to the
/// clipboard: some platforms serve clipboard data from the owning process.
pub struct SystemClipboard {
    inner: Mutex<Arboard>,
}

impl SystemClipboard {
    /// Connect to the platform clipboard.
    pub fn new() -> Result<Self, ClipboardError> {
        Arboard::new()
            .map(|clipboard| Self {
                inner: Mutex::new(clipboard),
            })
            .map_err(ClipboardError::Backend)
    }
}

impl Clipboard for SystemClipboard {
    fn get_text(&self) -> Result<Option<String>, ClipboardError> {
        let mut clipboard = self.inner.lock().map_err(|_| ClipboardError::Poisoned)?;
        match clipboard.get_text() {
            Ok(text) => Ok(Some(text)),
            Err(arboard::Error::ContentNotAvailable) => Ok(None),
            Err(error) => Err(ClipboardError::Backend(error)),
        }
    }

    fn set_text(&self, text: &str) -> Result<(), ClipboardError> {
        let mut clipboard = self.inner.lock().map_err(|_| ClipboardError::Poisoned)?;
        clipboard.set_text(text).map_err(ClipboardError::Backend)
    }
}

/// An in-memory clipboard for headless tests and non-desktop frontends.
#[derive(Debug, Default)]
pub struct MemoryClipboard {
    text: Mutex<Option<String>>,
}

impl MemoryClipboard {
    /// Create an empty in-memory clipboard.
    pub fn new() -> Self {
        Self::default()
    }

    /// Current text without going through the trait.
    pub fn snapshot(&self) -> Option<String> {
        self.text.lock().expect("memory clipboard lock").clone()
    }
}

impl Clipboard for MemoryClipboard {
    fn get_text(&self) -> Result<Option<String>, ClipboardError> {
        Ok(self
            .text
            .lock()
            .map_err(|_| ClipboardError::Poisoned)?
            .clone())
    }

    fn set_text(&self, text: &str) -> Result<(), ClipboardError> {
        *self.text.lock().map_err(|_| ClipboardError::Poisoned)? = Some(text.to_owned());
        Ok(())
    }
}

/// Clipboard access failure.
#[derive(Debug)]
pub enum ClipboardError {
    /// The platform backend failed.
    Backend(arboard::Error),
    /// An internal lock was poisoned after a panic.
    Poisoned,
}

impl fmt::Display for ClipboardError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Backend(error) => write!(formatter, "clipboard backend failed: {error}"),
            Self::Poisoned => write!(formatter, "clipboard state is poisoned"),
        }
    }
}

impl StdError for ClipboardError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Backend(error) => Some(error),
            Self::Poisoned => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_clipboard_round_trips_text() {
        let clipboard = MemoryClipboard::new();
        assert_eq!(clipboard.get_text().expect("read"), None);

        clipboard.set_text("hello").expect("write");
        assert_eq!(clipboard.get_text().expect("read"), Some("hello".into()));
        assert_eq!(clipboard.snapshot(), Some("hello".into()));

        clipboard.set_text("replaced").expect("write");
        assert_eq!(clipboard.get_text().expect("read"), Some("replaced".into()));
    }
}
