//! UTF-8 document storage, line indexing, and file round-tripping.

use std::{
    borrow::Cow,
    collections::VecDeque,
    fmt,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process,
    sync::atomic::{AtomicU64, Ordering},
};

use crate::{gap::GapBuffer, segmented::Segmented};

/// Default maximum file size accepted by [`Document::open`], in bytes.
pub const DEFAULT_OPEN_LIMIT: u64 = 64 * 1024 * 1024;

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// The line ending detected when a file was opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineEnding {
    Lf,
    CrLf,
}

/// A document read or write failure.
#[derive(Debug)]
pub struct DocumentError {
    path: PathBuf,
    operation: &'static str,
    message: String,
}

impl DocumentError {
    fn new(path: impl Into<PathBuf>, operation: &'static str, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            operation,
            message: message.into(),
        }
    }
}

impl fmt::Display for DocumentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "could not {} {}: {}",
            self.operation,
            self.path.display(),
            self.message
        )
    }
}

impl std::error::Error for DocumentError {}

/// A UTF-8 text document with a byte-offset line index.
#[derive(Clone, Debug)]
pub struct Document {
    buffer: GapBuffer,
    line_starts: Segmented<usize>,
    line_ending: LineEnding,
    path: Option<PathBuf>,
    bom: bool,
    revision: u64,
}

impl Document {
    /// Create an empty untitled document.
    pub fn empty() -> Self {
        Self::from_text("")
    }

    /// Create an in-memory LF document.
    pub fn from_text(text: &str) -> Self {
        let mut document = Self {
            buffer: GapBuffer::from_bytes(text.as_bytes()),
            line_starts: Segmented::from_vec(Vec::new()),
            line_ending: LineEnding::Lf,
            path: None,
            bom: false,
            revision: 0,
        };
        document.rebuild_lines();
        document
    }

    /// Open a UTF-8 file, strip a BOM, normalize line endings, and preserve the
    /// detected newline style for saving.
    ///
    /// Files larger than [`DEFAULT_OPEN_LIMIT`] are rejected; use
    /// [`Document::open_with_limit`] when the application has a different bound.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DocumentError> {
        Self::open_with_limit(path, DEFAULT_OPEN_LIMIT)
    }

    /// Open a UTF-8 file with an explicit upper bound in bytes.
    ///
    /// The file is never read past `limit + 1` bytes, so a file that grows
    /// while it is being opened cannot bypass the limit.
    pub fn open_with_limit(path: impl AsRef<Path>, limit: u64) -> Result<Self, DocumentError> {
        let path = path.as_ref();
        let file = File::open(path)
            .map_err(|error| DocumentError::new(path, "open", error.to_string()))?;
        if let Ok(metadata) = file.metadata()
            && metadata.len() > limit
        {
            return Err(too_large(path, limit));
        }

        let mut bytes = Vec::new();
        let mut limited = file.take(limit.saturating_add(1));
        limited
            .read_to_end(&mut bytes)
            .map_err(|error| DocumentError::new(path, "read", error.to_string()))?;
        if bytes.len() as u64 > limit {
            return Err(too_large(path, limit));
        }

        let (bom, bytes) = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
            (true, &bytes[3..])
        } else {
            (false, bytes.as_slice())
        };
        let text = std::str::from_utf8(bytes)
            .map_err(|error| DocumentError::new(path, "decode", error.to_string()))?;
        let line_ending = if text.contains("\r\n") {
            LineEnding::CrLf
        } else {
            LineEnding::Lf
        };
        let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
        let mut document = Self::from_text(&normalized);
        document.path = Some(path.to_path_buf());
        document.bom = bom;
        document.line_ending = line_ending;
        Ok(document)
    }

    /// Save the document, restoring the detected newline style and BOM.
    pub fn save(&self) -> Result<(), DocumentError> {
        let path = self
            .path
            .as_ref()
            .ok_or_else(|| DocumentError::new("", "save", "the document has no path"))?;
        self.save_as(path)
    }

    /// Save to a specific path.
    ///
    /// The write is staged in a temporary file in the destination directory and
    /// renamed into place, so a failed write does not truncate the old file.
    /// Existing ordinary permission bits are preserved; on Unix, a new file
    /// starts as `0600`.
    ///
    /// The inode is replaced: ownership, ACLs, xattrs, and hard links are not
    /// preserved by this atomic-save path.
    pub fn save_as(&self, path: impl AsRef<Path>) -> Result<(), DocumentError> {
        let path = path.as_ref();
        let text = self.text();
        let text = match self.line_ending {
            LineEnding::Lf => text,
            LineEnding::CrLf => text.replace('\n', "\r\n"),
        };
        let mut bytes = Vec::with_capacity(text.len() + 3);
        if self.bom {
            bytes.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
        }
        bytes.extend_from_slice(text.as_bytes());
        write_atomically(path, &bytes)
            .map_err(|error| DocumentError::new(path, "save", error.to_string()))
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn set_path(&mut self, path: impl Into<PathBuf>) {
        self.path = Some(path.into());
    }

    pub fn line_ending(&self) -> LineEnding {
        self.line_ending
    }

    pub fn set_line_ending(&mut self, line_ending: LineEnding) {
        self.line_ending = line_ending;
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    pub fn text(&self) -> String {
        String::from_utf8(self.buffer.to_vec()).expect("editor buffer is UTF-8")
    }

    pub fn read(&self, range: std::ops::Range<usize>) -> Vec<u8> {
        self.buffer.read(range)
    }

    /// Find substring matches by streaming the gap buffer, without copying the
    /// document into one contiguous string.
    pub fn find_matches(&self, query: &str, case_sensitive: bool) -> Vec<std::ops::Range<usize>> {
        let needle: Vec<char> = if case_sensitive {
            query.chars().collect()
        } else {
            query.chars().flat_map(char::to_lowercase).collect()
        };
        if needle.is_empty() {
            return Vec::new();
        }

        let mut failure = vec![0; needle.len()];
        for index in 1..needle.len() {
            let mut matched = failure[index - 1];
            while matched > 0 && needle[index] != needle[matched] {
                matched = failure[matched - 1];
            }
            if needle[index] == needle[matched] {
                matched += 1;
            }
            failure[index] = matched;
        }

        let mut matches = Vec::new();
        let mut window: VecDeque<(usize, usize)> = VecDeque::with_capacity(needle.len());
        let mut matched = 0;
        let mut last_end = 0;
        let mut feed = |character: char, start: usize, end: usize| {
            if start < last_end {
                return;
            }
            while matched > 0 && needle[matched] != character {
                matched = failure[matched - 1];
            }
            if needle[matched] == character {
                matched += 1;
            }
            window.push_back((start, end));
            if window.len() > needle.len() {
                window.pop_front();
            }
            if matched == needle.len() {
                let start = window.front().map_or(start, |(start, _)| *start);
                let end = window.back().map_or(end, |(_, end)| *end);
                matches.push(start..end);
                last_end = end;
                matched = 0;
                window.clear();
            }
        };

        self.buffer.visit_chars(|start, character, end| {
            if case_sensitive {
                feed(character, start, end);
            } else {
                for lower in character.to_lowercase() {
                    feed(lower, start, end);
                }
            }
        });
        matches
    }

    /// Whether `offset` is at a UTF-8 code-point boundary.
    pub fn is_char_boundary(&self, offset: usize) -> bool {
        if offset == 0 || offset >= self.len() {
            return offset <= self.len();
        }
        self.buffer
            .read(offset..offset + 1)
            .first()
            .is_some_and(|byte| byte & 0xC0 != 0x80)
    }

    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    pub fn line_start(&self, line: usize) -> usize {
        self.line_starts.get(line).unwrap_or(self.len())
    }

    pub fn line_end(&self, line: usize) -> usize {
        self.line_starts
            .get(line + 1)
            .map_or(self.len(), |start| start.saturating_sub(1))
    }

    /// Text of one logical line, excluding its newline.
    pub fn line_text(&self, line: usize) -> String {
        self.line_text_cow(line).into_owned()
    }

    /// Line text without copying when the line does not cross the gap.
    pub fn line_text_cow(&self, line: usize) -> Cow<'_, str> {
        if line >= self.line_count() {
            return Cow::Borrowed("");
        }
        let start = self.line_start(line);
        let end = self.line_end(line);
        let [left, right] = self.buffer.chunks();
        if end <= left.len() {
            return Cow::Borrowed(
                std::str::from_utf8(&left[start..end]).expect("editor buffer is UTF-8"),
            );
        }
        if start >= left.len() {
            let offset = left.len();
            return Cow::Borrowed(
                std::str::from_utf8(&right[start - offset..end - offset])
                    .expect("editor buffer is UTF-8"),
            );
        }
        Cow::Owned(String::from_utf8(self.buffer.read(start..end)).expect("editor buffer is UTF-8"))
    }

    pub fn line_of(&self, offset: usize) -> usize {
        let offset = offset.min(self.len());
        self.line_starts
            .partition_point(|start| start <= offset)
            .saturating_sub(1)
    }

    /// Replace a byte range. The range must already respect UTF-8 boundaries.
    pub fn replace(&mut self, range: std::ops::Range<usize>, insert: &str) {
        let removed = range.end - range.start;
        let delta = insert.len() as isize - removed as isize;

        // Keep the line index incremental: only line starts inside the edited
        // range are replaced, and later starts are shifted by the byte delta.
        // This avoids scanning the whole document on every keystroke.
        let prefix_end = self
            .line_starts
            .partition_point(|start| start < range.start);
        let suffix_start = self.line_starts.partition_point(|start| start <= range.end);
        let has_start_at_range = self.line_starts.get(prefix_end) == Some(range.start);

        self.buffer.replace(range.clone(), insert.as_bytes());

        let mut inserted =
            Vec::with_capacity(insert.matches('\n').count() + usize::from(has_start_at_range));
        if has_start_at_range {
            inserted.push(range.start);
        }
        inserted.extend(
            insert
                .match_indices('\n')
                .map(|(index, _)| range.start + index + 1),
        );
        self.line_starts
            .replace(prefix_end, suffix_start, inserted, 0, delta);
        self.revision = self.revision.wrapping_add(1);
    }

    fn rebuild_lines(&mut self) {
        let mut starts = vec![0];
        let mut offset = 0;
        for chunk in self.buffer.chunks() {
            for byte in chunk {
                offset += 1;
                if *byte == b'\n' {
                    starts.push(offset);
                }
            }
        }
        self.line_starts = Segmented::from_vec(starts);
    }
}

fn too_large(path: &Path, limit: u64) -> DocumentError {
    DocumentError::new(path, "open", format!("file is larger than {limit} bytes"))
}

fn write_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let target = resolve_target(path)?;
    if target.file_name().is_none() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "save path has no file name",
        ));
    }
    let parent = target
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    #[cfg(unix)]
    let mode = temporary_mode(&target);
    #[cfg(not(unix))]
    let mode = 0;
    let (temporary, mut file) = create_temporary(parent, mode)?;

    let written = (|| -> io::Result<()> {
        file.write_all(bytes)?;
        if let Ok(metadata) = fs::metadata(&target) {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;

                // Writing a file normally clears setuid/setgid bits. Keep the
                // ordinary permission bits, but do not reintroduce privilege
                // bits through the atomic replacement.
                let permissions = fs::Permissions::from_mode(metadata.permissions().mode() & 0o777);
                file.set_permissions(permissions)?;
            }
            #[cfg(not(unix))]
            file.set_permissions(metadata.permissions())?;
        }
        file.sync_all()
    })();
    if let Err(error) = written {
        drop(file);
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    drop(file);

    if let Err(error) = fs::rename(&temporary, &target) {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    sync_parent(parent);
    Ok(())
}

fn resolve_target(path: &Path) -> io::Result<PathBuf> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => fs::canonicalize(path),
        Ok(_) => Ok(path.to_path_buf()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(path.to_path_buf()),
        Err(error) => Err(error),
    }
}

#[cfg(unix)]
fn temporary_mode(target: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;

    fs::metadata(target)
        .map(|metadata| metadata.permissions().mode() & 0o777)
        .unwrap_or(0o600)
}

fn create_temporary(parent: &Path, mode: u32) -> io::Result<(PathBuf, File)> {
    #[cfg(not(unix))]
    let _ = mode;

    for _ in 0..32 {
        let candidate = parent.join(format!(
            ".ferriso-save-{}-{}.tmp",
            process::id(),
            TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;

            options.mode(mode);
        }
        match options.open(&candidate) {
            Ok(file) => return Ok((candidate, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not create a unique temporary file",
    ))
}

#[cfg(unix)]
fn sync_parent(parent: &Path) {
    if let Ok(directory) = File::open(parent) {
        let _ = directory.sync_all();
    }
}

#[cfg(not(unix))]
fn sync_parent(_parent: &Path) {}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streaming_search_matches_across_the_gap_and_unicode_case() {
        let mut document = Document::from_text("Café café CAFÉ\n😀 alpha");
        document.buffer.replace(3..3, b"");
        assert_eq!(
            document.find_matches("café", false),
            vec![0..5, 6..11, 12..17]
        );
        assert_eq!(document.find_matches("CAFÉ", true), vec![12..17]);
        assert_eq!(document.find_matches("alpha", true), vec![23..28]);
        assert!(document.find_matches("", false).is_empty());
    }

    #[test]
    fn streaming_search_matches_a_plain_document_at_every_gap_position() {
        let text = "aé中😀z alpha café CAFÉ";
        let expected_plain = Document::from_text(text);
        for gap in 0..=text.len() {
            let mut document = Document::from_text(text);
            document.buffer.replace(gap..gap, b"");
            for query in ["é", "😀", "alpha", "café", "CAFÉ", "aa"] {
                for case_sensitive in [false, true] {
                    assert_eq!(
                        document.find_matches(query, case_sensitive),
                        expected_plain.find_matches(query, case_sensitive),
                        "gap {gap}, query {query:?}, case {case_sensitive}"
                    );
                }
            }
        }
    }

    #[test]
    fn streaming_search_is_non_overlapping() {
        let document = Document::from_text("aaaa");
        assert_eq!(document.find_matches("aa", true), vec![0..2, 2..4]);
    }

    #[test]
    fn borrowed_line_text_falls_back_across_the_gap() {
        let mut document = Document::from_text("alpha\nbeta");
        document.replace(4..4, "x");
        assert_eq!(document.line_text_cow(0), "alphxa");
        assert_eq!(document.line_text_cow(1), "beta");
        assert_eq!(document.line_text_cow(2), "");
        assert_eq!(document.line_text_cow(9), "");
    }

    #[test]
    fn line_index_includes_a_final_empty_line() {
        let document = Document::from_text("one\ntwo\n");
        assert_eq!(document.line_count(), 3);
        assert_eq!(document.line_text(0), "one");
        assert_eq!(document.line_text(1), "two");
        assert_eq!(document.line_text(2), "");
        assert_eq!(document.line_of(document.len()), 2);
    }

    #[test]
    fn segmented_line_index_matches_a_full_rebuild_across_chunks() {
        let text = (0..700)
            .map(|line| format!("line {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut document = Document::from_text(&text);
        let inserts = ["", "x", "\n", "yz", "\n\n"];
        let mut state = 0xfeed_face_u64;

        for _ in 0..400 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let start = (state as usize) % (document.len() + 1);
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let end = start + ((state as usize) % (document.len() + 1 - start));
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let insert = inserts[(state as usize) % inserts.len()];

            document.replace(start..end, insert);

            let mut rebuilt = document.clone();
            rebuilt.rebuild_lines();
            assert_eq!(
                document.line_starts.to_vec(),
                rebuilt.line_starts.to_vec(),
                "range {start}..{end} insert {insert:?}"
            );
            assert_eq!(document.line_of(document.len()), document.line_count() - 1);
        }
    }

    #[test]
    fn incremental_line_index_matches_a_full_rebuild() {
        let mut document = Document::from_text("alpha\nbeta\ngamma\n");
        let inserts = ["", "x", "\n", "yz", "\n\n"];
        let mut state = 0x9e37_79b9_u64;

        for _ in 0..500 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let start = (state as usize) % (document.len() + 1);
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let end = start + ((state as usize) % (document.len() + 1 - start));
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let insert = inserts[(state as usize) % inserts.len()];

            document.replace(start..end, insert);

            let mut rebuilt = document.clone();
            rebuilt.rebuild_lines();
            assert_eq!(
                document.line_starts.to_vec(),
                rebuilt.line_starts.to_vec(),
                "range {start}..{end} insert {insert:?}"
            );
        }
    }

    #[test]
    fn replacement_updates_line_index() {
        let mut document = Document::from_text("alpha\nbeta");
        document.replace(5..6, "beta\n");
        assert_eq!(document.text(), "alphabeta\nbeta");
        assert_eq!(document.line_count(), 2);
        document.replace(0..0, "X\n");
        assert_eq!(document.line_text(0), "X");
        assert_eq!(document.line_of(2), 1);
    }

    #[test]
    fn bom_and_crlf_survive_a_round_trip() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("sample.txt");
        std::fs::write(&path, b"\xEF\xBB\xBFone\r\ntwo\r\n").expect("write fixture");
        let mut document = Document::open(&path).expect("open");
        assert_eq!(document.text(), "one\ntwo\n");
        assert_eq!(document.line_ending(), LineEnding::CrLf);
        document.replace(3..3, "!");
        document.save().expect("save");
        let bytes = std::fs::read(&path).expect("read");
        assert_eq!(bytes, b"\xEF\xBB\xBFone!\r\ntwo\r\n");
    }

    #[test]
    fn invalid_utf8_is_reported() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("invalid.bin");
        std::fs::write(&path, [0xFF, 0xFE]).expect("write fixture");
        let error = Document::open(&path).expect_err("invalid UTF-8 must fail");
        assert!(error.to_string().contains("decode"));
    }
    #[test]
    fn save_replaces_content_without_leaving_a_temp_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("note.txt");
        fs::write(&path, b"old").expect("write fixture");

        Document::from_text("new")
            .save_as(&path)
            .expect("atomic save");

        assert_eq!(fs::read(&path).expect("read"), b"new");
        assert!(
            fs::read_dir(dir.path())
                .expect("read dir")
                .all(|entry| !entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".ferriso-save-")),
            "temporary save file must be removed"
        );
    }

    #[test]
    fn open_rejects_files_above_the_explicit_limit() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("large.txt");
        fs::write(&path, b"12345").expect("write fixture");

        let rejected = Document::open_with_limit(&path, 4).expect_err("limit must reject");
        assert!(rejected.to_string().contains("larger than 4 bytes"));

        let accepted = Document::open_with_limit(&path, 5).expect("exact limit is allowed");
        assert_eq!(accepted.text(), "12345");
    }

    #[cfg(unix)]
    #[test]
    fn save_preserves_the_existing_file_mode() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("mode.txt");
        fs::write(&path, b"old").expect("write fixture");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).expect("set mode");

        Document::from_text("new")
            .save_as(&path)
            .expect("atomic save");

        let mode = fs::metadata(&path).expect("metadata").permissions().mode() & 0o777;
        assert_eq!(mode, 0o640);
    }
    #[cfg(unix)]
    #[test]
    fn temporary_save_files_start_with_a_private_mode() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("temp dir");
        let target = dir.path().join("target.txt");
        fs::write(&target, b"old").expect("write fixture");
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).expect("set mode");

        let mode = temporary_mode(&target);
        assert_eq!(mode, 0o600);
        let (temporary, file) = create_temporary(dir.path(), mode).expect("temporary file");
        assert_eq!(
            fs::metadata(&temporary)
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );

        drop(file);
        fs::remove_file(temporary).expect("remove temporary");
    }
}
