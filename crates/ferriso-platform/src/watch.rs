//! File watching with normalized events and a deterministic memory backend.

use std::{
    error::Error as StdError,
    fmt,
    path::{Path, PathBuf},
    pin::Pin,
    sync::Mutex,
    task::{Context, Poll},
};

use futures_core::Stream;
use notify::{
    Event, EventKind, RecursiveMode, Watcher,
    event::{ModifyKind, RenameMode},
};
use tokio::sync::mpsc;

/// Maximum queued events per watch. A full queue drops new notifications
/// instead of growing without bound.
const WATCH_CAPACITY: usize = 1024;

/// One normalized filesystem change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WatchEvent {
    /// A path was created.
    Created(PathBuf),
    /// File content or metadata changed.
    Changed(PathBuf),
    /// A path was removed.
    Removed(PathBuf),
    /// A path was renamed.
    Renamed {
        /// Previous path.
        from: PathBuf,
        /// New path.
        to: PathBuf,
    },
    /// A backend event that does not fit the categories above.
    Other(PathBuf),
    /// The backend reported a non-fatal error.
    Error(String),
}

/// Start file watchers.
pub trait FileWatcher: Send + Sync + 'static {
    /// Watch `path`, optionally including descendants.
    fn watch(&self, path: &Path, recursive: bool) -> Result<Watch, WatchError>;
}

/// A live watch. Event delivery is bounded; when the consumer falls behind,
/// newer events are dropped instead of growing memory without bound. Dropping
/// the watch stops delivery and releases the backend watcher.
pub struct Watch {
    events: mpsc::Receiver<WatchEvent>,
    _watcher: Option<notify::RecommendedWatcher>,
}

impl Stream for Watch {
    type Item = WatchEvent;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.events.poll_recv(context)
    }
}

impl Watch {
    /// Wait for the next event. Returns `None` after the watch is closed.
    pub async fn next(&mut self) -> Option<WatchEvent> {
        self.events.recv().await
    }

    /// Take a queued event without waiting.
    pub fn try_next(&mut self) -> Option<WatchEvent> {
        self.events.try_recv().ok()
    }
}

/// The host filesystem watcher through [`notify`].
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemFileWatcher;

impl SystemFileWatcher {
    /// Create a stateless system watcher.
    pub const fn new() -> Self {
        Self
    }
}

impl FileWatcher for SystemFileWatcher {
    fn watch(&self, path: &Path, recursive: bool) -> Result<Watch, WatchError> {
        let (sender, events) = mpsc::channel(WATCH_CAPACITY);
        let watcher =
            notify::recommended_watcher(move |result: Result<Event, notify::Error>| match result {
                Ok(event) => {
                    for event in normalize(event) {
                        let _ = sender.try_send(event);
                    }
                }
                Err(error) => {
                    let _ = sender.try_send(WatchEvent::Error(error.to_string()));
                }
            })
            .map_err(WatchError::Backend)?;

        let mut watcher = watcher;
        watcher
            .watch(path, recursive_mode(recursive))
            .map_err(WatchError::Backend)?;

        Ok(Watch {
            events,
            _watcher: Some(watcher),
        })
    }
}

/// A deterministic watcher for tests; [`MemoryFileWatcher::emit`] feeds all
/// active watches.
#[derive(Debug, Default)]
pub struct MemoryFileWatcher {
    senders: Mutex<Vec<mpsc::Sender<WatchEvent>>>,
}

impl MemoryFileWatcher {
    /// Create a watcher with no active watches.
    pub fn new() -> Self {
        Self::default()
    }

    /// Send one event to every active watch.
    pub fn emit(&self, event: WatchEvent) {
        let Ok(mut senders) = self.senders.lock() else {
            return;
        };
        senders.retain(|sender| {
            !matches!(
                sender.try_send(event.clone()),
                Err(mpsc::error::TrySendError::Closed(_))
            )
        });
    }
}

impl FileWatcher for MemoryFileWatcher {
    fn watch(&self, _path: &Path, _recursive: bool) -> Result<Watch, WatchError> {
        let (sender, events) = mpsc::channel(WATCH_CAPACITY);
        self.senders
            .lock()
            .map_err(|_| WatchError::Poisoned)?
            .push(sender);
        Ok(Watch {
            events,
            _watcher: None,
        })
    }
}

/// Failure to create or configure a watch.
#[derive(Debug)]
pub enum WatchError {
    /// The notify backend rejected the watch.
    Backend(notify::Error),
    /// An internal lock was poisoned after a panic.
    Poisoned,
}

impl fmt::Display for WatchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Backend(error) => write!(formatter, "watch backend failed: {error}"),
            Self::Poisoned => write!(formatter, "watch state is poisoned"),
        }
    }
}

impl StdError for WatchError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Backend(error) => Some(error),
            Self::Poisoned => None,
        }
    }
}

fn recursive_mode(recursive: bool) -> RecursiveMode {
    if recursive {
        RecursiveMode::Recursive
    } else {
        RecursiveMode::NonRecursive
    }
}

fn normalize(event: Event) -> Vec<WatchEvent> {
    match event.kind {
        EventKind::Create(_) => event.paths.into_iter().map(WatchEvent::Created).collect(),
        EventKind::Remove(_) => event.paths.into_iter().map(WatchEvent::Removed).collect(),
        EventKind::Modify(ModifyKind::Name(RenameMode::Both)) if event.paths.len() >= 2 => {
            vec![WatchEvent::Renamed {
                from: event.paths[0].clone(),
                to: event.paths[1].clone(),
            }]
        }
        EventKind::Modify(_) => event.paths.into_iter().map(WatchEvent::Changed).collect(),
        EventKind::Access(_) => Vec::new(),
        _ => event.paths.into_iter().map(WatchEvent::Other).collect(),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::time::timeout;

    use super::*;

    #[tokio::test]
    async fn memory_watcher_emits_normalized_events() {
        let watcher = MemoryFileWatcher::new();
        let mut watch = watcher.watch(Path::new("/tmp"), true).expect("watch");

        watcher.emit(WatchEvent::Created(PathBuf::from("/tmp/file")));
        watcher.emit(WatchEvent::Renamed {
            from: PathBuf::from("/tmp/old"),
            to: PathBuf::from("/tmp/new"),
        });

        assert_eq!(
            timeout(Duration::from_secs(1), watch.next())
                .await
                .expect("first event"),
            Some(WatchEvent::Created(PathBuf::from("/tmp/file")))
        );
        assert_eq!(
            timeout(Duration::from_secs(1), watch.next())
                .await
                .expect("second event"),
            Some(WatchEvent::Renamed {
                from: PathBuf::from("/tmp/old"),
                to: PathBuf::from("/tmp/new"),
            })
        );
    }

    #[test]
    fn watch_can_feed_a_send_stream() {
        fn assert_send<T: Send>() {}
        fn assert_stream<T: Stream>() {}

        assert_send::<Watch>();
        assert_stream::<Watch>();
    }

    #[test]
    fn memory_watcher_can_be_injected_as_a_trait_object() {
        let watcher: Box<dyn FileWatcher> = Box::new(MemoryFileWatcher::new());
        assert!(watcher.watch(Path::new("/tmp"), false).is_ok());
    }

    #[tokio::test]
    async fn system_watcher_reports_a_new_file() {
        use std::{
            env, fs,
            time::{SystemTime, UNIX_EPOCH},
        };

        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock after epoch")
            .as_nanos();
        let directory =
            env::temp_dir().join(format!("ferriso-watch-{}-{stamp}", std::process::id()));
        fs::create_dir_all(&directory).expect("create watch directory");

        let watcher = SystemFileWatcher::new();
        let mut watch = watcher
            .watch(&directory, false)
            .expect("start system watcher");

        let file = directory.join("created.txt");
        fs::write(&file, "watch me").expect("create file");

        let found = timeout(Duration::from_secs(5), async {
            loop {
                match watch.next().await {
                    Some(WatchEvent::Created(path))
                    | Some(WatchEvent::Changed(path))
                    | Some(WatchEvent::Other(path))
                        if path.ends_with("created.txt") =>
                    {
                        break true;
                    }
                    Some(WatchEvent::Error(error)) => panic!("watch failed: {error}"),
                    Some(_) => continue,
                    None => break false,
                }
            }
        })
        .await
        .expect("watch event arrives");

        let _ = fs::remove_dir_all(&directory);
        assert!(found, "new file event was observed");
    }
}
