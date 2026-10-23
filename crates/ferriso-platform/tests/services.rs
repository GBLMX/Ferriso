#![cfg(all(feature = "dialogs", feature = "notifications"))]

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use ferriso_platform::{FileDialog, MemoryFileDialog, MemoryNotifier, Notification, Notifier};

#[test]
fn notifications_and_dialogs_are_injectable_trait_objects() {
    let notifier: Arc<dyn Notifier> = Arc::new(MemoryNotifier::new());
    notifier
        .notify(&Notification::new("Build", "finished"))
        .expect("notify");

    let dialog: Arc<dyn FileDialog> = Arc::new(
        MemoryFileDialog::new()
            .with_file(Path::new("/tmp/input"))
            .with_folder(Path::new("/tmp/folder"))
            .with_save(Path::new("/tmp/output")),
    );

    assert_eq!(dialog.pick_file(), Some(PathBuf::from("/tmp/input")));
    assert_eq!(dialog.pick_folder(), Some(PathBuf::from("/tmp/folder")));
    assert_eq!(dialog.save_file(), Some(PathBuf::from("/tmp/output")));
}
