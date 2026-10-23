use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use ferriso_platform::{OpenError, Opener};

#[derive(Default)]
struct RecordingOpener {
    opened: Mutex<Vec<PathBuf>>,
}

impl Opener for RecordingOpener {
    fn open(&self, target: &Path) -> Result<(), OpenError> {
        self.opened
            .lock()
            .expect("opener lock")
            .push(target.to_path_buf());
        Ok(())
    }
}

#[test]
fn applications_can_inject_an_opener_through_the_trait() {
    let recorder = Arc::new(RecordingOpener::default());
    let opener: Arc<dyn Opener> = recorder.clone();

    opener.open(Path::new("/tmp/first")).expect("open first");
    opener.open(Path::new("/tmp/second")).expect("open second");

    assert_eq!(
        *recorder.opened.lock().expect("opener lock"),
        [PathBuf::from("/tmp/first"), PathBuf::from("/tmp/second")]
    );
}
