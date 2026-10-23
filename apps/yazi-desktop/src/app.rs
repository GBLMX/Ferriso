//! The yazi-desktop domain model.
//!
//! A file manager that reads the filesystem asynchronously, keeps navigation
//! history, filters the current listing, and previews the selection. All state
//! lives behind the Ferriso actor; the iced and terminal frontends only render
//! snapshots and send [`Message`]s.

use std::{
    env, fs,
    io::Read,
    path::{Component, Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime},
};

use ferriso_core::{App, Command};
use ferriso_platform::{Opener, SystemOpener};
use tokio::sync::Semaphore;

use crate::preview::{CacheKey, PreviewCache, PreviewImage, decode_image};

/// Maximum number of bytes read for a text preview.
pub const PREVIEW_LIMIT: u64 = 64 * 1024;

/// Maximum number of names shown in a directory preview.
const PREVIEW_NAMES: usize = 14;

/// File families used for labels, colors, and the orbit chart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Dir,
    Code,
    Text,
    Image,
    Audio,
    Video,
    Archive,
    Binary,
    Other,
}

impl Kind {
    pub fn from_name(name: &str) -> Self {
        let extension = Path::new(name)
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();

        match extension.as_str() {
            "rs" | "py" | "js" | "mjs" | "ts" | "tsx" | "jsx" | "c" | "h" | "cc" | "cpp"
            | "hpp" | "go" | "java" | "kt" | "lua" | "sh" | "zsh" | "fish" | "css" | "html"
            | "json" | "yaml" | "yml" | "toml" | "wgsl" | "glsl" | "sql" | "nix" | "zig" => {
                Self::Code
            }
            "md" | "txt" | "rst" | "log" | "csv" | "license" | "lock" => Self::Text,
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "ico" | "bmp" | "avif" => Self::Image,
            "mp3" | "flac" | "wav" | "ogg" | "m4a" | "opus" => Self::Audio,
            "mp4" | "mkv" | "webm" | "mov" | "avi" => Self::Video,
            "zip" | "tar" | "gz" | "xz" | "zst" | "7z" | "rar" | "bz2" => Self::Archive,
            "bin" | "so" | "dll" | "exe" | "a" | "o" | "wasm" | "rlib" => Self::Binary,
            _ => Self::Other,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Dir => "dir",
            Self::Code => "code",
            Self::Text => "text",
            Self::Image => "image",
            Self::Audio => "audio",
            Self::Video => "video",
            Self::Archive => "archive",
            Self::Binary => "binary",
            Self::Other => "file",
        }
    }

    /// Index into [`Stats::buckets`] and the preview orbit.
    pub fn bucket(self) -> usize {
        match self {
            Self::Dir => 0,
            Self::Code => 1,
            Self::Text => 2,
            Self::Image | Self::Audio | Self::Video => 3,
            Self::Archive | Self::Binary | Self::Other => 4,
        }
    }
}

/// One directory entry.
#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub kind: Kind,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub hidden: bool,
}

/// Content shown in the preview pane.
#[derive(Clone, Debug)]
pub struct Preview {
    pub path: PathBuf,
    pub title: String,
    pub subtitle: String,
    pub meta: Vec<(String, String)>,
    pub body: String,
    pub is_text: bool,
    pub truncated: bool,
    /// Monotonic selection token; stale async results carry an older value.
    pub generation: u64,
    /// Width the image was decoded for, in logical pixels.
    pub target_width: u32,
    pub modified: Option<SystemTime>,
    /// Decoded RGBA, present only for supported images.
    pub image: Option<Arc<PreviewImage>>,
}

impl Preview {
    pub fn empty() -> Self {
        Self {
            path: PathBuf::new(),
            title: "Nothing selected".into(),
            subtitle: "Navigate with the list or the keyboard".into(),
            meta: Vec::new(),
            body: String::new(),
            is_text: false,
            truncated: false,
            generation: 0,
            target_width: 0,
            modified: None,
            image: None,
        }
    }

    fn loading(path: &Path, title: &str, generation: u64, target_width: u32) -> Self {
        Self {
            path: path.to_path_buf(),
            title: title.to_owned(),
            subtitle: "Reading…".into(),
            meta: Vec::new(),
            body: String::new(),
            is_text: false,
            truncated: false,
            generation,
            target_width,
            modified: None,
            image: None,
        }
    }

    fn failed(path: &Path, error: String, generation: u64) -> Self {
        Self {
            path: path.to_path_buf(),
            title: path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string()),
            subtitle: "Unavailable".into(),
            meta: vec![("Error".into(), error)],
            body: String::new(),
            is_text: false,
            truncated: false,
            generation,
            target_width: 0,
            modified: None,
            image: None,
        }
    }

    fn cache_key(&self) -> Option<CacheKey> {
        self.image.as_ref()?;
        Some(CacheKey::new(&self.path, self.modified, self.target_width))
    }
}

/// Aggregate counts for the status bar and the orbit chart.
#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub dirs: usize,
    pub files: usize,
    pub bytes: u64,
    /// dirs, code, text, media, other
    pub buckets: [usize; 5],
}

/// A clickable path segment.
#[derive(Clone, Debug)]
pub struct Crumb {
    pub label: String,
    pub path: PathBuf,
    pub current: bool,
}

/// A sidebar shortcut.
#[derive(Clone, Debug)]
pub struct PlaceRow {
    pub label: &'static str,
    pub path: PathBuf,
    pub active: bool,
}

/// One rendered file row.
#[derive(Clone, Debug)]
pub struct Row {
    pub name: String,
    pub detail: String,
    pub kind: Kind,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub hidden: bool,
    pub selected: bool,
}

/// Immutable snapshot consumed by the iced view.
#[derive(Clone, Debug)]
pub struct ViewModel {
    pub cwd: String,
    pub crumbs: Vec<Crumb>,
    pub places: Vec<PlaceRow>,
    pub rows: Vec<Row>,
    pub selected: Option<usize>,
    pub filter: String,
    pub show_hidden: bool,
    pub loading: bool,
    pub status: String,
    pub error: bool,
    pub preview: Preview,
    pub stats: Stats,
}

/// Semantic input accepted by the file manager.
#[derive(Clone, Debug)]
pub enum Message {
    Open(PathBuf),
    Up,
    Back,
    Home,
    Refresh,
    Select(usize),
    MoveSelection(i32),
    Activate,
    FilterChanged(String),
    ToggleHidden,
    Loaded { path: PathBuf, entries: Vec<Entry> },
    LoadFailed { path: PathBuf, error: String },
    PreviewLoaded(Option<Preview>),
    PreviewWidth(u32),
    Opened(Result<(), String>),
}

/// The file manager application.
pub struct Yazi {
    cwd: PathBuf,
    home: PathBuf,
    entries: Vec<Entry>,
    filter: String,
    selected: Option<usize>,
    history: Vec<PathBuf>,
    show_hidden: bool,
    preview: Preview,
    preview_generation: Arc<AtomicU64>,
    preview_width: u32,
    preview_cache: PreviewCache,
    image_slots: Arc<Semaphore>,
    status: String,
    error: bool,
    loading: bool,
}

impl Yazi {
    pub fn new(start: PathBuf, show_hidden: bool) -> Self {
        let home = env::var_os("HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"));
        let cwd = if start.is_dir() { start } else { home.clone() };

        Self {
            cwd,
            home,
            entries: Vec::new(),
            filter: String::new(),
            selected: None,
            history: Vec::new(),
            show_hidden,
            preview: Preview::empty(),
            preview_generation: Arc::new(AtomicU64::new(0)),
            preview_width: 360,
            preview_cache: PreviewCache::new(),
            image_slots: Arc::new(Semaphore::new(1)),
            status: "Loading…".into(),
            error: false,
            loading: true,
        }
    }

    fn visible(&self, entry: &Entry) -> bool {
        let filter = self.filter.trim();
        filter.is_empty() || entry.name.to_lowercase().contains(&filter.to_lowercase())
    }

    fn visible_len(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| self.visible(entry))
            .count()
    }

    fn selected_entry(&self) -> Option<&Entry> {
        let index = self.selected?;
        self.entries
            .iter()
            .filter(|entry| self.visible(entry))
            .nth(index)
    }

    fn load(&mut self) -> Command<Message> {
        self.loading = true;
        self.error = false;
        self.status = "Loading…".into();
        let path = self.cwd.clone();
        let request = path.clone();
        let show_hidden = self.show_hidden;

        Command::perform(
            async move {
                match tokio::task::spawn_blocking(move || read_directory(&request, show_hidden))
                    .await
                {
                    Ok(Ok(entries)) => Message::Loaded { path, entries },
                    Ok(Err(error)) => Message::LoadFailed { path, error },
                    Err(error) => Message::LoadFailed {
                        path,
                        error: error.to_string(),
                    },
                }
            },
            |message| message,
        )
    }

    fn open_path(&mut self, path: PathBuf) -> Command<Message> {
        self.cwd = path;
        self.filter.clear();
        self.selected = None;
        self.preview = Preview::empty();
        self.load()
    }

    fn refresh_preview(&mut self) -> Command<Message> {
        let generation = self.preview_generation.fetch_add(1, Ordering::SeqCst) + 1;
        let Some(entry) = self.selected_entry().cloned() else {
            self.preview = Preview::empty();
            return Command::none();
        };

        self.preview = Preview::loading(&entry.path, &entry.name, generation, self.preview_width);
        let path = entry.path.clone();
        let width = self.preview_width;
        let cached = self
            .preview_cache
            .get(&CacheKey::new(&path, entry.modified, width));
        let current = Arc::clone(&self.preview_generation);
        let slots = Arc::clone(&self.image_slots);
        let needs_decode = entry.kind == Kind::Image && cached.is_none();

        Command::perform(
            async move {
                // Coalesce rapid keyboard navigation before touching the disk.
                tokio::time::sleep(Duration::from_millis(80)).await;
                if current.load(Ordering::SeqCst) != generation {
                    return Message::PreviewLoaded(None);
                }

                // One decode at a time keeps rapid navigation from piling up
                // full-size images in memory. Re-check after waiting: a newer
                // selection makes this decode pointless.
                let _permit = if needs_decode {
                    let Ok(permit) = slots.acquire().await else {
                        return Message::PreviewLoaded(None);
                    };
                    if current.load(Ordering::SeqCst) != generation {
                        return Message::PreviewLoaded(None);
                    }
                    Some(permit)
                } else {
                    None
                };

                let failure_path = path.clone();
                let result = tokio::task::spawn_blocking(move || {
                    read_preview_sized(&path, width, generation, cached)
                })
                .await;

                Message::PreviewLoaded(Some(match result {
                    Ok(preview) => preview,
                    Err(error) => Preview::failed(&failure_path, error.to_string(), generation),
                }))
            },
            |message| message,
        )
    }

    fn apply_loaded(&mut self, path: PathBuf, entries: Vec<Entry>) -> Command<Message> {
        if path != self.cwd {
            return Command::none();
        }

        self.entries = entries;
        self.loading = false;
        self.error = false;
        self.selected = (self.visible_len() > 0).then_some(0);
        self.status = self.status_line();
        self.refresh_preview()
    }

    fn status_line(&self) -> String {
        let stats = stats(&self.entries);
        let shown = self.visible_len();
        if self.filter.trim().is_empty() {
            format!(
                "{} items · {} dirs · {} files · {}",
                shown,
                stats.dirs,
                stats.files,
                format_size(stats.bytes)
            )
        } else {
            format!("{shown} matches · filter “{}”", self.filter.trim())
        }
    }

    fn move_selection(&mut self, delta: i32) -> Command<Message> {
        let len = self.visible_len();
        if len == 0 {
            self.selected = None;
            self.preview = Preview::empty();
            return Command::none();
        }

        let current = self.selected.unwrap_or(0) as i32;
        let next = (current + delta).clamp(0, len as i32 - 1) as usize;
        if self.selected == Some(next) {
            return Command::none();
        }

        self.selected = Some(next);
        self.refresh_preview()
    }

    fn view_model(&self) -> ViewModel {
        let rows = self
            .entries
            .iter()
            .filter(|entry| self.visible(entry))
            .enumerate()
            .map(|(index, entry)| Row {
                name: entry.name.clone(),
                detail: if entry.is_dir {
                    format!(
                        "dir · {}{}",
                        if entry.is_symlink { "link · " } else { "" },
                        entry
                            .modified
                            .map(format_modified)
                            .unwrap_or_else(|| "unknown".into())
                    )
                } else {
                    format!(
                        "{} · {}{} · {}",
                        entry.kind.label(),
                        format_size(entry.size),
                        if entry.is_symlink { " · link" } else { "" },
                        entry
                            .modified
                            .map(format_modified)
                            .unwrap_or_else(|| "unknown".into())
                    )
                },
                kind: entry.kind,
                is_dir: entry.is_dir,
                is_symlink: entry.is_symlink,
                hidden: entry.hidden,
                selected: self.selected == Some(index),
            })
            .collect();

        ViewModel {
            cwd: display_path(&self.cwd, &self.home),
            crumbs: crumbs(&self.cwd, &self.home),
            places: places(&self.home, &self.cwd),
            rows,
            selected: self.selected,
            filter: self.filter.clone(),
            show_hidden: self.show_hidden,
            loading: self.loading,
            status: self.status.clone(),
            error: self.error,
            preview: self.preview.clone(),
            stats: stats(&self.entries),
        }
    }
}

impl App for Yazi {
    type Message = Message;
    type View = ViewModel;

    fn init(&mut self) -> Command<Message> {
        self.load()
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::Open(path) => {
                if path == self.cwd {
                    return self.load();
                }
                if self.cwd.is_dir() {
                    self.history.push(self.cwd.clone());
                }
                self.open_path(path)
            }
            Message::Up => match self.cwd.parent() {
                Some(parent) => {
                    self.history.push(self.cwd.clone());
                    self.open_path(parent.to_path_buf())
                }
                None => Command::none(),
            },
            Message::Back => match self.history.pop() {
                Some(path) => self.open_path(path),
                None => match self.cwd.parent() {
                    Some(parent) => self.open_path(parent.to_path_buf()),
                    None => Command::none(),
                },
            },
            Message::Home => {
                if self.cwd != self.home {
                    self.history.push(self.cwd.clone());
                    self.open_path(self.home.clone())
                } else {
                    Command::none()
                }
            }
            Message::Refresh => self.load(),
            Message::Select(index) => {
                if index >= self.visible_len() || self.selected == Some(index) {
                    return Command::none();
                }
                self.selected = Some(index);
                self.refresh_preview()
            }
            Message::MoveSelection(delta) => self.move_selection(delta),
            Message::Activate => {
                let Some(entry) = self.selected_entry().cloned() else {
                    return Command::none();
                };
                if entry.is_dir {
                    if self.cwd.is_dir() {
                        self.history.push(self.cwd.clone());
                    }
                    self.open_path(entry.path)
                } else {
                    open_external(entry.path)
                }
            }
            Message::FilterChanged(filter) => {
                self.filter = filter;
                self.selected = (self.visible_len() > 0).then_some(0);
                self.status = self.status_line();
                self.refresh_preview()
            }
            Message::ToggleHidden => {
                self.show_hidden = !self.show_hidden;
                self.load()
            }
            Message::Loaded { path, entries } => self.apply_loaded(path, entries),
            Message::LoadFailed { path, error } => {
                if path != self.cwd {
                    return Command::none();
                }
                self.loading = false;
                self.error = true;
                self.status = error;
                self.entries.clear();
                self.selected = None;
                self.preview = Preview::empty();
                Command::none()
            }
            Message::PreviewWidth(width) => {
                let width = width.clamp(160, 2048);
                if width.abs_diff(self.preview_width) < 24 {
                    return Command::none();
                }
                self.preview_width = width;
                if self
                    .selected_entry()
                    .is_some_and(|entry| entry.kind == Kind::Image)
                {
                    self.refresh_preview()
                } else {
                    Command::none()
                }
            }
            Message::PreviewLoaded(Some(preview)) => {
                let matches = preview.generation == self.preview_generation.load(Ordering::SeqCst)
                    && self
                        .selected_entry()
                        .is_some_and(|entry| entry.path == preview.path);
                if matches {
                    if let (Some(key), Some(image)) = (preview.cache_key(), preview.image.clone()) {
                        self.preview_cache.insert(key, image);
                    }
                    self.preview = preview;
                }
                Command::none()
            }
            Message::PreviewLoaded(None) => Command::none(),
            Message::Opened(result) => {
                self.error = result.is_err();
                self.status = match result {
                    Ok(()) => "Opened with the system handler".into(),
                    Err(error) => error,
                };
                Command::none()
            }
        }
    }

    fn view(&self) -> ViewModel {
        self.view_model()
    }

    fn debug(&self, message: &Message) -> Option<String> {
        Some(match message {
            Message::Open(path) => format!("Open({})", path.display()),
            Message::Up => "Up".into(),
            Message::Back => "Back".into(),
            Message::Home => "Home".into(),
            Message::Refresh => "Refresh".into(),
            Message::Select(index) => format!("Select({index})"),
            Message::MoveSelection(delta) => format!("MoveSelection({delta})"),
            Message::Activate => "Activate".into(),
            Message::FilterChanged(filter) => format!("FilterChanged({filter:?})"),
            Message::ToggleHidden => "ToggleHidden".into(),
            Message::Loaded { path, entries } => {
                format!("Loaded({}, {} entries)", path.display(), entries.len())
            }
            Message::LoadFailed { path, error } => {
                format!("LoadFailed({}, {error:?})", path.display())
            }
            Message::PreviewLoaded(Some(preview)) => format!(
                "PreviewLoaded({}, {} bytes, image={})",
                preview.path.display(),
                preview.body.len(),
                preview.image.is_some()
            ),
            Message::PreviewLoaded(None) => "PreviewLoaded(stale)".into(),
            Message::PreviewWidth(width) => format!("PreviewWidth({width})"),
            Message::Opened(result) => match result {
                Ok(()) => "Opened(ok)".into(),
                Err(error) => format!("Opened(error: {error})"),
            },
        })
    }
}

fn open_external(path: PathBuf) -> Command<Message> {
    Command::perform(
        async move {
            let result = tokio::task::spawn_blocking(move || SystemOpener::new().open(&path))
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result.map_err(|error| error.to_string()));
            Message::Opened(result)
        },
        |message| message,
    )
}

/// Read one directory into a sorted, filtered listing.
pub fn read_directory(path: &Path, show_hidden: bool) -> Result<Vec<Entry>, String> {
    let mut entries = Vec::new();

    for entry in fs::read_dir(path).map_err(|error| format!("{}: {error}", path.display()))? {
        let Ok(entry) = entry else { continue };
        let name = entry.file_name().to_string_lossy().into_owned();
        let hidden = name.starts_with('.');
        if hidden && !show_hidden {
            continue;
        }

        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let is_symlink = file_type.is_symlink();
        let metadata = if is_symlink {
            fs::metadata(entry.path()).ok()
        } else {
            entry.metadata().ok()
        };
        let is_dir = file_type.is_dir()
            || (is_symlink && metadata.as_ref().is_some_and(|meta| meta.is_dir()));
        let kind = if is_dir {
            Kind::Dir
        } else {
            Kind::from_name(&name)
        };

        entries.push(Entry {
            name,
            path: entry.path(),
            kind,
            is_dir,
            is_symlink,
            size: metadata.as_ref().map(|meta| meta.len()).unwrap_or(0),
            modified: metadata.and_then(|meta| meta.modified().ok()),
            hidden,
        });
    }

    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.name.cmp(&b.name))
    });

    Ok(entries)
}

/// Read a file or directory preview without decoding images.
///
/// Used by tests and by callers that do not have a preview width yet.
pub fn read_preview(path: &Path) -> Preview {
    read_preview_sized(path, 0, 0, None)
}

/// Read a preview, optionally decoding an image at `target_width`.
fn read_preview_sized(
    path: &Path,
    target_width: u32,
    generation: u64,
    cached: Option<Arc<PreviewImage>>,
) -> Preview {
    let title = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());

    let Ok(metadata) = fs::metadata(path) else {
        return Preview::failed(path, "metadata unavailable".into(), generation);
    };
    let modified = metadata.modified().ok();

    let mut preview = Preview {
        path: path.to_path_buf(),
        title,
        subtitle: if metadata.is_dir() {
            "Directory".into()
        } else {
            Kind::from_name(&path.to_string_lossy()).label().into()
        },
        meta: Vec::new(),
        body: String::new(),
        is_text: false,
        truncated: false,
        generation,
        target_width,
        modified,
        image: None,
    };

    if !metadata.is_dir() {
        preview
            .meta
            .push(("Size".into(), format_size(metadata.len())));
    }
    if let Some(modified) = modified {
        preview
            .meta
            .push(("Modified".into(), format_modified(modified)));
    }

    if metadata.is_dir() {
        match directory_summary(path) {
            Ok((names, dirs, files, total)) => {
                preview.meta.push((
                    "Items".into(),
                    format!("{total} · {dirs} dirs · {files} files"),
                ));
                preview.body = names.join("\n");
                if total > names.len() {
                    preview.body.push_str("\n…");
                }
            }
            Err(error) => preview.meta.push(("Error".into(), error)),
        }
        return preview;
    }

    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if extension == "pdf" {
        preview.subtitle = "PDF".into();
        preview.body = "PDF first-page preview is not enabled in this build.".into();
        preview.meta.push(("Preview".into(), "deferred".into()));
        return preview;
    }

    if Kind::from_name(&path.to_string_lossy()) == Kind::Image {
        preview.subtitle = "Image".into();
        let image = match cached {
            Some(image) => Some(image),
            None if target_width > 0 => match decode_image(path, target_width) {
                Ok(image) => Some(Arc::new(image)),
                Err(error) => {
                    preview.meta.push(("Error".into(), error));
                    preview.body = "Image preview unavailable.".into();
                    return preview;
                }
            },
            None => None,
        };

        if let Some(image) = image {
            preview.subtitle = "Image".into();
            preview.meta.push((
                "Preview".into(),
                format!("{} × {}", image.width, image.height),
            ));
            preview.image = Some(image);
            return preview;
        }
    }

    if metadata.len() > PREVIEW_LIMIT {
        preview.truncated = true;
    }

    match read_text(path, PREVIEW_LIMIT) {
        Ok(Some(text)) => {
            preview.is_text = true;
            preview.body = text;
        }
        Ok(None) => {
            preview.body = "Binary content is not previewed.".into();
        }
        Err(error) => preview.meta.push(("Error".into(), error)),
    }

    preview
}

fn directory_summary(path: &Path) -> Result<(Vec<String>, usize, usize, usize), String> {
    let mut names = Vec::new();
    let mut dirs = 0;
    let mut files = 0;

    for entry in fs::read_dir(path).map_err(|error| error.to_string())? {
        let Ok(entry) = entry else { continue };
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let is_dir = entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false);
        if is_dir {
            dirs += 1;
        } else {
            files += 1;
        }
        if names.len() < PREVIEW_NAMES {
            names.push(if is_dir { format!("{name}/") } else { name });
        }
    }

    names.sort_by_key(|name| name.to_lowercase());

    Ok((names, dirs, files, dirs + files))
}

fn read_text(path: &Path, limit: u64) -> Result<Option<String>, String> {
    let file = fs::File::open(path).map_err(|error| error.to_string())?;
    let mut buffer = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut buffer)
        .map_err(|error| error.to_string())?;

    if buffer.len() as u64 > limit {
        buffer.truncate(limit as usize);
    }
    if buffer.contains(&0) {
        return Ok(None);
    }

    String::from_utf8(buffer)
        .map(Some)
        .map_err(|_| "File is not valid UTF-8".into())
}

/// Aggregates for the status bar and orbit chart.
pub fn stats(entries: &[Entry]) -> Stats {
    let mut stats = Stats::default();
    for entry in entries {
        if entry.is_dir {
            stats.dirs += 1;
        } else {
            stats.files += 1;
            stats.bytes += entry.size;
        }
        stats.buckets[entry.kind.bucket()] += 1;
    }
    stats
}

/// Breadcrumb segments, collapsing the home prefix to `~`.
pub fn crumbs(path: &Path, home: &Path) -> Vec<Crumb> {
    let mut crumbs = Vec::new();

    if let Ok(rest) = path.strip_prefix(home) {
        crumbs.push(Crumb {
            label: "~".into(),
            path: home.to_path_buf(),
            current: rest.as_os_str().is_empty(),
        });
        let mut accumulated = home.to_path_buf();
        for component in rest.components() {
            accumulated.push(component.as_os_str());
            crumbs.push(Crumb {
                label: component.as_os_str().to_string_lossy().into_owned(),
                path: accumulated.clone(),
                current: accumulated == path,
            });
        }
    } else {
        let mut accumulated = PathBuf::new();
        for component in path.components() {
            accumulated.push(component.as_os_str());
            let label = match component {
                Component::RootDir => "/".into(),
                _ => component.as_os_str().to_string_lossy().into_owned(),
            };
            crumbs.push(Crumb {
                label,
                path: accumulated.clone(),
                current: accumulated == path,
            });
        }
    }

    crumbs
}

/// Sidebar shortcuts that exist on this machine.
pub fn places(home: &Path, cwd: &Path) -> Vec<PlaceRow> {
    let mut candidates: Vec<(&'static str, PathBuf)> = vec![
        ("Home", home.to_path_buf()),
        ("Desktop", home.join("Desktop")),
        ("Documents", home.join("Documents")),
        ("Downloads", home.join("Downloads")),
        ("Pictures", home.join("Pictures")),
        ("Music", home.join("Music")),
        ("Videos", home.join("Videos")),
        ("Projects", home.join("Projects")),
        ("Root", PathBuf::from("/")),
    ];
    candidates.retain(|(_, path)| path.is_dir());

    candidates
        .into_iter()
        .map(|(label, path)| PlaceRow {
            label,
            active: cwd == path,
            path,
        })
        .collect()
}

/// Shorten a path with `~` when it lives under home.
pub fn display_path(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".into(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

/// Human-readable byte size.
pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Coarse relative modification time.
pub fn format_modified(time: SystemTime) -> String {
    let Ok(age) = SystemTime::now().duration_since(time) else {
        return "in the future".into();
    };
    let seconds = age.as_secs();

    match seconds {
        0..=59 => "just now".into(),
        60..=3_599 => format!("{} min ago", seconds / 60),
        3_600..=86_399 => format!("{} h ago", seconds / 3_600),
        _ => format!("{} d ago", seconds / 86_400),
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("yazi-desktop-{tag}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("create temp dir");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn directories_sort_first_then_case_insensitively() {
        let dir = TempDir::new("sort");
        fs::create_dir(dir.path().join("zdir")).unwrap();
        fs::write(dir.path().join("Beta.rs"), "").unwrap();
        fs::write(dir.path().join("alpha.md"), "").unwrap();

        let entries = read_directory(dir.path(), false).expect("read");
        let names: Vec<_> = entries.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(names, ["zdir", "alpha.md", "Beta.rs"]);
        assert!(entries[0].is_dir);
        assert_eq!(entries[1].kind, Kind::Text);
        assert_eq!(entries[2].kind, Kind::Code);
    }

    #[test]
    fn hidden_entries_need_the_flag() {
        let dir = TempDir::new("hidden");
        fs::write(dir.path().join(".secret"), "").unwrap();
        fs::write(dir.path().join("visible"), "").unwrap();

        assert_eq!(read_directory(dir.path(), false).unwrap().len(), 1);
        assert_eq!(read_directory(dir.path(), true).unwrap().len(), 2);
    }

    #[test]
    fn kinds_cover_common_families() {
        assert_eq!(Kind::from_name("main.rs"), Kind::Code);
        assert_eq!(Kind::from_name("README.md"), Kind::Text);
        assert_eq!(Kind::from_name("cover.PNG"), Kind::Image);
        assert_eq!(Kind::from_name("song.flac"), Kind::Audio);
        assert_eq!(Kind::from_name("clip.mkv"), Kind::Video);
        assert_eq!(Kind::from_name("archive.tar"), Kind::Archive);
        assert_eq!(Kind::from_name("lib.so"), Kind::Binary);
        assert_eq!(Kind::from_name("LICENSE"), Kind::Other);
        assert_eq!(Kind::Dir.bucket(), 0);
        assert_eq!(Kind::Image.bucket(), 3);
    }

    #[test]
    fn preview_reads_text_and_reports_binaries() {
        let dir = TempDir::new("preview");
        let text = dir.path().join("notes.txt");
        fs::write(&text, "hello ferriso").unwrap();
        let preview = read_preview(&text);
        assert!(preview.is_text);
        assert_eq!(preview.body, "hello ferriso");
        assert_eq!(preview.subtitle, "text");

        let binary = dir.path().join("data.bin");
        fs::write(&binary, [0_u8, 1, 2, 3]).unwrap();
        let preview = read_preview(&binary);
        assert!(!preview.is_text);
        assert!(preview.body.contains("Binary"));
    }

    #[test]
    fn crumbs_collapse_home_and_keep_full_paths() {
        let home = PathBuf::from("/home/test");
        let path = PathBuf::from("/home/test/Projects/Ferriso");
        let crumbs = crumbs(&path, &home);
        let labels: Vec<_> = crumbs.iter().map(|crumb| crumb.label.as_str()).collect();
        assert_eq!(labels, ["~", "Projects", "Ferriso"]);
        assert_eq!(crumbs[0].path, home);
        assert!(crumbs[2].current);
    }

    #[test]
    fn sizes_are_human_readable() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(1023), "1023 B");
        assert_eq!(format_size(2048), "2.0 KiB");
        assert_eq!(format_size(5 * 1024 * 1024), "5.0 MiB");
    }

    async fn wait_for(
        snapshots: &mut ferriso_core::Snapshots<ViewModel>,
        mut predicate: impl FnMut(&ViewModel) -> bool,
    ) -> ViewModel {
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            loop {
                let view = snapshots.changed().await.expect("actor alive");
                if predicate(&view) {
                    break (*view).clone();
                }
            }
        })
        .await
        .expect("view condition")
    }

    #[test]
    fn image_preview_decodes_at_the_requested_width() {
        let dir = TempDir::new("image-preview");
        let path = dir.path().join("wide.png");
        let image = image::RgbaImage::from_pixel(400, 200, image::Rgba([20, 180, 160, 255]));
        image.save(&path).unwrap();

        let preview = read_preview_sized(&path, 120, 7, None);
        let decoded = preview.image.expect("decoded image");
        assert_eq!(preview.generation, 7);
        assert_eq!(preview.target_width, 120);
        assert_eq!(decoded.width, 120);
        assert_eq!(decoded.height, 60);
        assert_eq!(decoded.rgba.len(), 120 * 60 * 4);
        assert_eq!(preview.subtitle, "Image");
    }

    #[test]
    fn broken_images_fall_back_to_a_visible_message() {
        let dir = TempDir::new("broken-image");
        let path = dir.path().join("broken.png");
        fs::write(&path, b"not a png").unwrap();

        let preview = read_preview_sized(&path, 120, 9, None);
        assert_eq!(preview.generation, 9);
        assert_eq!(preview.subtitle, "Image");
        assert!(preview.image.is_none());
        assert!(preview.body.contains("unavailable"));
        assert!(preview.meta.iter().any(|(key, _)| key == "Error"));
    }

    #[test]
    fn pdf_preview_is_an_explicit_placeholder() {
        let dir = TempDir::new("pdf-preview");
        let path = dir.path().join("paper.pdf");
        fs::write(&path, b"%PDF-1.7\n").unwrap();

        let preview = read_preview(&path);
        assert_eq!(preview.subtitle, "PDF");
        assert!(preview.body.contains("not enabled"));
        assert!(preview.image.is_none());
    }

    #[tokio::test]
    async fn actor_lists_filters_and_navigates() {
        let dir = TempDir::new("actor");
        fs::create_dir(dir.path().join("sub")).unwrap();
        fs::write(dir.path().join("alpha.rs"), "fn main() {}").unwrap();
        fs::write(dir.path().join("beta.md"), "# b").unwrap();
        fs::write(dir.path().join(".hidden"), "x").unwrap();

        let (handle, mut snapshots, task) =
            ferriso_core::spawn(Yazi::new(dir.path().to_path_buf(), false));

        let view = wait_for(&mut snapshots, |view| !view.loading && view.rows.len() == 3).await;
        let names: Vec<_> = view.rows.iter().map(|row| row.name.as_str()).collect();
        assert_eq!(names, ["sub", "alpha.rs", "beta.md"]);
        assert_eq!(view.stats.dirs, 1);
        assert_eq!(view.stats.files, 2);

        handle.send(Message::FilterChanged("RS".into()));
        let view = wait_for(&mut snapshots, |view| view.rows.len() == 1).await;
        assert_eq!(view.rows[0].name, "alpha.rs");
        assert!(view.status.contains("matches"));

        handle.send(Message::Open(dir.path().join("sub")));
        let view = wait_for(&mut snapshots, |view| {
            !view.loading && view.crumbs.last().is_some_and(|crumb| crumb.label == "sub")
        })
        .await;
        assert!(view.rows.is_empty());
        assert_eq!(view.filter, "");

        handle.send(Message::Back);
        let view = wait_for(&mut snapshots, |view| view.rows.len() == 3).await;
        assert_eq!(view.rows[0].name, "sub");

        handle.shutdown();
        tokio::time::timeout(std::time::Duration::from_secs(3), task.join())
            .await
            .expect("actor exits");
    }
}
