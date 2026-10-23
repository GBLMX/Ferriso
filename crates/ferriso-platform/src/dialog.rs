//! Native file and folder pickers with an in-memory test double.

use std::{path::PathBuf, sync::Mutex};

/// Pick files and folders through the platform dialog service.
///
/// `None` means the user cancelled or the platform could not present a dialog.
/// The synchronous system backend may block; run it from a blocking task when
/// used on an async UI thread.
pub trait FileDialog: Send + Sync + 'static {
    /// Pick one existing file.
    fn pick_file(&self) -> Option<PathBuf>;

    /// Pick one existing folder.
    fn pick_folder(&self) -> Option<PathBuf>;

    /// Pick a destination path for saving.
    fn save_file(&self) -> Option<PathBuf>;
}

/// The host desktop dialog through [`rfd`].
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemFileDialog;

impl SystemFileDialog {
    /// Create a stateless system dialog service.
    pub const fn new() -> Self {
        Self
    }
}

impl FileDialog for SystemFileDialog {
    fn pick_file(&self) -> Option<PathBuf> {
        rfd::FileDialog::new().pick_file()
    }

    fn pick_folder(&self) -> Option<PathBuf> {
        rfd::FileDialog::new().pick_folder()
    }

    fn save_file(&self) -> Option<PathBuf> {
        rfd::FileDialog::new().save_file()
    }
}

/// A deterministic dialog double for headless tests.
#[derive(Debug, Default)]
pub struct MemoryFileDialog {
    file: Mutex<Option<PathBuf>>,
    folder: Mutex<Option<PathBuf>>,
    save: Mutex<Option<PathBuf>>,
}

impl MemoryFileDialog {
    /// Create a dialog where every picker returns `None`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the next file picker result.
    pub fn with_file(self, path: impl Into<PathBuf>) -> Self {
        *self.file.lock().expect("file dialog lock") = Some(path.into());
        self
    }

    /// Set the next folder picker result.
    pub fn with_folder(self, path: impl Into<PathBuf>) -> Self {
        *self.folder.lock().expect("folder dialog lock") = Some(path.into());
        self
    }

    /// Set the next save picker result.
    pub fn with_save(self, path: impl Into<PathBuf>) -> Self {
        *self.save.lock().expect("save dialog lock") = Some(path.into());
        self
    }
}

impl FileDialog for MemoryFileDialog {
    fn pick_file(&self) -> Option<PathBuf> {
        self.file.lock().ok().and_then(|path| path.clone())
    }

    fn pick_folder(&self) -> Option<PathBuf> {
        self.folder.lock().ok().and_then(|path| path.clone())
    }

    fn save_file(&self) -> Option<PathBuf> {
        self.save.lock().ok().and_then(|path| path.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_dialog_returns_configured_paths() {
        let dialog = MemoryFileDialog::new()
            .with_file("/tmp/file")
            .with_folder("/tmp/folder")
            .with_save("/tmp/save");

        assert_eq!(dialog.pick_file(), Some(PathBuf::from("/tmp/file")));
        assert_eq!(dialog.pick_folder(), Some(PathBuf::from("/tmp/folder")));
        assert_eq!(dialog.save_file(), Some(PathBuf::from("/tmp/save")));
    }
}
