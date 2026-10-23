//! Editing state: cursor, selection, transactions, scrolling, and view model.

use std::{
    borrow::Cow,
    ops::Range,
    sync::mpsc::{self, Receiver, TryRecvError},
};

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use lsh::definitions::HighlightKind;
use lsh::runtime::Highlight;

use crate::{
    document::{Document, DocumentError},
    highlight::{Highlighter, language_for_path},
    segmented::{Segmented, Shift},
    view::{EditorView, TokenKind, VisualLine, VisualRun},
};

const DEFAULT_ROWS: usize = 40;
const DEFAULT_COLS: usize = 100;
const DEFAULT_TAB_WIDTH: usize = 4;
/// Above this size, a width change builds the new layout off the UI thread.
const DEFERRED_WRAP_LINES: usize = 4_096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CursorState {
    offset: usize,
    anchor: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditKind {
    Insert,
    Other,
}

#[derive(Clone, Debug)]
struct Change {
    range: Range<usize>,
    removed: Vec<u8>,
    inserted: Vec<u8>,
}

#[derive(Clone, Debug)]
struct Transaction {
    id: u64,
    changes: Vec<Change>,
    before: CursorState,
    after: CursorState,
    kind: EditKind,
}

#[derive(Default)]
struct History {
    undo: Vec<Transaction>,
    redo: Vec<Transaction>,
    next_id: u64,
    saved: Option<Vec<u64>>,
    merge_blocked: bool,
}

impl History {
    fn push(&mut self, mut transaction: Transaction) {
        self.redo.clear();

        if !self.merge_blocked
            && transaction.kind == EditKind::Insert
            && transaction.changes.len() == 1
            && self
                .undo
                .last()
                .is_some_and(|last| last.kind == EditKind::Insert)
        {
            let change = &transaction.changes[0];
            let can_merge = self.undo.last().is_some_and(|last| {
                last.after == transaction.before
                    && last.changes.len() == 1
                    && last.changes[0].range.start + last.changes[0].inserted.len()
                        == change.range.start
                    && !change.inserted.contains(&b'\n')
            });
            if can_merge {
                let last = self.undo.last_mut().expect("checked above");
                last.changes[0].inserted.extend_from_slice(&change.inserted);
                last.after = transaction.after;
                return;
            }
        }

        transaction.id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.undo.push(transaction);
        self.merge_blocked = false;
    }

    fn block_merge(&mut self) {
        self.merge_blocked = true;
    }

    fn is_dirty(&self) -> bool {
        let Some(saved) = &self.saved else {
            return true;
        };
        saved.len() != self.undo.len()
            || saved
                .iter()
                .zip(&self.undo)
                .any(|(saved_id, transaction)| *saved_id != transaction.id)
    }

    fn mark_saved(&mut self) {
        self.saved = Some(self.undo.iter().map(|transaction| transaction.id).collect());
        self.merge_blocked = true;
    }

    fn reset(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.next_id = 0;
        self.saved = None;
        self.merge_blocked = false;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WrapRow {
    line: usize,
    start: usize,
    end: usize,
    start_column: usize,
    width: usize,
    continuation: bool,
}

struct WrapCache {
    revision: u64,
    width: usize,
    tab_width: usize,
    rows: WrapRows,
}

struct WrapPending {
    width: usize,
    tab_width: usize,
    revision: u64,
    anchor_line: usize,
    anchor_offset: usize,
    rows: Receiver<Vec<WrapRow>>,
}

/// Rows are stored in segmented storage; edits adjust suffix deltas instead
/// of shifting every wrapped row.
struct WrapRows {
    rows: Segmented<WrapRow>,
}

impl Shift for WrapRow {
    fn shifted(self, line_delta: isize, byte_delta: isize) -> Self {
        Self {
            line: (self.line as isize + line_delta) as usize,
            start: (self.start as isize + byte_delta) as usize,
            end: (self.end as isize + byte_delta) as usize,
            ..self
        }
    }
}

impl WrapRows {
    fn from_rows(rows: Vec<WrapRow>) -> Self {
        Self {
            rows: Segmented::from_vec(rows),
        }
    }

    fn len(&self) -> usize {
        self.rows.len()
    }

    fn get(&self, index: usize) -> Option<WrapRow> {
        self.rows.get(index)
    }

    #[cfg(test)]
    fn to_vec(&self) -> Vec<WrapRow> {
        self.rows.to_vec()
    }

    #[cfg(test)]
    fn segment_count(&self) -> usize {
        self.rows.segment_count()
    }

    fn range(&self, start: usize, end: usize) -> Vec<WrapRow> {
        (start..end.min(self.len()))
            .filter_map(|index| self.get(index))
            .collect()
    }

    fn first_row_of_line(&self, line: usize) -> usize {
        self.rows.partition_point(|row| row.line < line)
    }

    fn replace(
        &mut self,
        prefix_end: usize,
        suffix_start: usize,
        dirty: Vec<WrapRow>,
        line_delta: isize,
        byte_delta: isize,
    ) {
        self.rows
            .replace(prefix_end, suffix_start, dirty, line_delta, byte_delta);
    }

    fn row_for_offset(&self, offset: usize, document: &Document) -> usize {
        let len = self.len();
        if len == 0 {
            return 0;
        }
        let low = self.rows.partition_point(|row| row.end < offset);
        if low >= len {
            return len - 1;
        }
        let row = self.get(low).expect("row index in range");
        if offset < row.end {
            return low;
        }
        if row.end == document.line_end(row.line) {
            return low;
        }
        if self
            .get(low + 1)
            .is_some_and(|next| next.line == row.line && next.start == offset)
        {
            return low + 1;
        }
        low
    }
}

/// The last contiguous edit, expressed in line coordinates. It lets the wrap
/// cache reuse the untouched prefix and suffix instead of re-wrapping the
/// whole document on every keystroke.
#[derive(Clone, Copy, Debug)]
struct WrapEdit {
    first_old_line: usize,
    last_old_line: usize,
    last_new_line: usize,
    line_delta: isize,
    byte_delta: isize,
}

/// A GUI-independent editor.
pub struct Editor {
    document: Document,
    cursor: usize,
    anchor: Option<usize>,
    preferred_column: Option<usize>,
    history: History,
    highlighter: Highlighter,
    scroll_row: usize,
    scroll_col: usize,
    viewport_rows: usize,
    viewport_cols: usize,
    tab_width: usize,
    wrap: bool,
    wrap_cache: Option<WrapCache>,
    wrap_pending: Option<WrapPending>,
    wrap_edit: Option<WrapEdit>,
    #[cfg(test)]
    wrap_full_rebuilds: usize,
    status: String,
}

impl Editor {
    /// Create an editor for an in-memory document.
    pub fn new(document: Document) -> Self {
        let language = document.path().and_then(language_for_path);
        let mut editor = Self {
            document,
            cursor: 0,
            anchor: None,
            preferred_column: None,
            history: History::default(),
            highlighter: Highlighter::new(language),
            scroll_row: 0,
            scroll_col: 0,
            viewport_rows: DEFAULT_ROWS,
            viewport_cols: DEFAULT_COLS,
            tab_width: DEFAULT_TAB_WIDTH,
            wrap: false,
            wrap_cache: None,
            wrap_pending: None,
            wrap_edit: None,
            #[cfg(test)]
            wrap_full_rebuilds: 0,
            status: String::new(),
        };
        editor.history.mark_saved();
        editor.ensure_cursor_visible();
        editor
    }

    /// Create an empty untitled editor.
    pub fn empty() -> Self {
        Self::new(Document::empty())
    }

    /// Open a file into a new editor.
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self, DocumentError> {
        Ok(Self::new(Document::open(path)?))
    }

    pub fn document(&self) -> &Document {
        &self.document
    }

    pub fn path(&self) -> Option<&std::path::Path> {
        self.document.path()
    }

    pub fn dirty(&self) -> bool {
        self.history.is_dirty()
    }

    pub fn cursor_offset(&self) -> usize {
        self.cursor
    }

    pub fn cursor_line(&self) -> usize {
        self.document.line_of(self.cursor)
    }

    pub fn cursor_column(&self) -> usize {
        let line = self.cursor_line();
        self.byte_to_visual(line, self.cursor)
    }

    pub fn selected_text(&self) -> Option<String> {
        let range = self.selection()?;
        Some(String::from_utf8(self.document.read(range)).expect("editor buffer is UTF-8"))
    }

    pub fn copy(&self) -> Option<String> {
        if let Some(text) = self.selected_text() {
            return Some(text);
        }
        let line = self.cursor_line();
        let start = self.document.line_start(line);
        let end = self.document.line_end(line);
        let end = if end < self.document.len() {
            end + 1
        } else {
            end
        };
        if start == end {
            None
        } else {
            Some(String::from_utf8(self.document.read(start..end)).expect("editor buffer is UTF-8"))
        }
    }

    pub fn cut(&mut self) -> Option<String> {
        let text = self.copy()?;
        if let Some(range) = self.selection() {
            self.delete_range(range, EditKind::Other);
        } else {
            let line = self.cursor_line();
            let start = self.document.line_start(line);
            let mut end = self.document.line_end(line);
            if end < self.document.len() {
                end += 1;
            }
            if start < end {
                self.delete_range(start..end, EditKind::Other);
            }
        }
        Some(text)
    }

    pub fn paste(&mut self, text: &str) {
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        self.insert_text_with_kind(&text, EditKind::Other);
    }

    pub fn insert_text(&mut self, text: &str) {
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        self.insert_text_with_kind(&text, EditKind::Insert);
    }

    pub fn backspace(&mut self) {
        if let Some(range) = self.selection() {
            self.delete_range(range, EditKind::Other);
            return;
        }
        if self.cursor == 0 {
            return;
        }
        let start = self.previous_grapheme(self.cursor);
        self.delete_range(start..self.cursor, EditKind::Other);
    }

    pub fn delete_forward(&mut self) {
        if let Some(range) = self.selection() {
            self.delete_range(range, EditKind::Other);
            return;
        }
        if self.cursor >= self.document.len() {
            return;
        }
        let end = self.next_grapheme(self.cursor);
        self.delete_range(self.cursor..end, EditKind::Other);
    }

    pub fn enter(&mut self) {
        self.insert_text("\n");
    }

    pub fn tab(&mut self) {
        let column = self.cursor_column();
        let spaces = self.tab_width - (column % self.tab_width);
        self.insert_text(&" ".repeat(spaces));
    }

    pub fn undo(&mut self) {
        let Some(transaction) = self.history.undo.pop() else {
            return;
        };
        let line = transaction
            .changes
            .first()
            .map_or(0, |change| self.document.line_of(change.range.start));
        for change in transaction.changes.iter().rev() {
            let end = change.range.start + change.inserted.len();
            let removed =
                String::from_utf8(change.removed.clone()).expect("editor history is UTF-8");
            self.document.replace(change.range.start..end, &removed);
        }
        self.highlighter.invalidate_from(line);
        self.cursor = transaction.before.offset.min(self.document.len());
        self.anchor = transaction.before.anchor;
        self.preferred_column = None;
        self.history.redo.push(transaction);
        self.history.block_merge();
        self.wrap_cache = None;
        self.wrap_edit = None;
        self.ensure_cursor_visible();
    }

    pub fn redo(&mut self) {
        let Some(transaction) = self.history.redo.pop() else {
            return;
        };
        let line = transaction
            .changes
            .first()
            .map_or(0, |change| self.document.line_of(change.range.start));
        for change in &transaction.changes {
            let inserted =
                String::from_utf8(change.inserted.clone()).expect("editor history is UTF-8");
            self.document.replace(change.range.clone(), &inserted);
        }
        self.highlighter.invalidate_from(line);
        self.cursor = transaction.after.offset.min(self.document.len());
        self.anchor = transaction.after.anchor;
        self.preferred_column = None;
        self.history.undo.push(transaction);
        self.history.block_merge();
        self.wrap_cache = None;
        self.wrap_edit = None;
        self.ensure_cursor_visible();
    }

    pub fn select_all(&mut self) {
        self.anchor = Some(0);
        self.cursor = self.document.len();
        self.preferred_column = None;
        self.ensure_cursor_visible();
    }

    pub fn move_left(&mut self, extend: bool) {
        let offset = self.previous_grapheme(self.cursor);
        self.set_cursor(offset, extend);
        self.preferred_column = None;
    }

    pub fn move_right(&mut self, extend: bool) {
        let offset = self.next_grapheme(self.cursor);
        self.set_cursor(offset, extend);
        self.preferred_column = None;
    }

    pub fn move_up(&mut self, extend: bool) {
        self.move_vertical(-1, extend);
    }

    pub fn move_down(&mut self, extend: bool) {
        self.move_vertical(1, extend);
    }

    pub fn move_page_up(&mut self, extend: bool) {
        self.move_vertical(-(self.viewport_rows as isize).max(1), extend);
    }

    pub fn move_page_down(&mut self, extend: bool) {
        self.move_vertical((self.viewport_rows as isize).max(1), extend);
    }

    pub fn move_home(&mut self, extend: bool) {
        if self.wrap {
            let row = self.wrap_row_for_offset(self.cursor);
            if let Some(layout) = self.wrap_row(row) {
                self.set_cursor(layout.start, extend);
                self.preferred_column = None;
            }
            return;
        }
        let line = self.cursor_line();
        self.set_cursor(self.document.line_start(line), extend);
        self.preferred_column = None;
    }

    pub fn move_end(&mut self, extend: bool) {
        if self.wrap {
            let row = self.wrap_row_for_offset(self.cursor);
            if let Some(layout) = self.wrap_row(row) {
                self.set_cursor(layout.end, extend);
                self.preferred_column = None;
            }
            return;
        }
        let line = self.cursor_line();
        self.set_cursor(self.document.line_end(line), extend);
        self.preferred_column = None;
    }

    pub fn move_document_start(&mut self, extend: bool) {
        self.set_cursor(0, extend);
        self.preferred_column = None;
    }

    pub fn move_document_end(&mut self, extend: bool) {
        self.set_cursor(self.document.len(), extend);
        self.preferred_column = None;
    }

    pub fn click(&mut self, row: usize, column: usize, extend: bool) {
        if self.wrap {
            self.ensure_wrap_cache();
            let layout = self.wrap_cache.as_ref().and_then(|cache| {
                cache
                    .rows
                    .get((self.scroll_row + row).min(cache.rows.len().saturating_sub(1)))
            });
            if let Some(layout) = layout {
                let target = layout.start_column + column;
                let offset = self
                    .visual_to_offset(layout.line, target)
                    .clamp(layout.start, layout.end);
                self.set_cursor(offset, extend);
                self.preferred_column = None;
                self.ensure_cursor_visible();
            }
            return;
        }
        let line = (self.scroll_row + row).min(self.document.line_count() - 1);
        let offset = self.visual_to_offset(line, self.scroll_col + column);
        self.set_cursor(offset, extend);
        self.preferred_column = None;
        self.ensure_cursor_visible();
    }

    pub fn scroll(&mut self, columns: isize, rows: isize) {
        if self.wrap {
            self.ensure_wrap_cache();
            let total = self
                .wrap_cache
                .as_ref()
                .map_or(1, |cache| cache.rows.len().max(1));
            self.scroll_row = (self.scroll_row as isize + rows)
                .clamp(0, total.saturating_sub(1) as isize) as usize;
            self.scroll_col = 0;
            return;
        }
        let last_line = self.document.line_count().saturating_sub(1);
        self.scroll_row = (self.scroll_row as isize + rows).clamp(0, last_line as isize) as usize;
        self.scroll_col = (self.scroll_col as isize + columns).max(0) as usize;
    }

    pub fn set_viewport(&mut self, rows: usize, columns: usize) {
        let columns = columns.max(1);
        if self.viewport_cols != columns {
            let tab_width = self.tab_width.max(1);
            let cache_matches = self
                .wrap_cache
                .as_ref()
                .is_some_and(|cache| cache.width == columns && cache.tab_width == tab_width);
            if self.wrap
                && self.document.line_count() >= DEFERRED_WRAP_LINES
                && self.wrap_cache.is_some()
            {
                if cache_matches {
                    self.wrap_pending = None;
                } else {
                    self.start_wrap_job(columns, tab_width);
                }
            } else {
                self.wrap_cache = None;
                self.wrap_pending = None;
                self.wrap_edit = None;
            }
        }
        self.viewport_rows = rows.max(1);
        self.viewport_cols = columns;
        self.ensure_cursor_visible();
    }

    pub fn wrap(&self) -> bool {
        self.wrap
    }

    pub fn set_wrap(&mut self, wrap: bool) {
        if self.wrap == wrap {
            return;
        }
        self.wrap = wrap;
        self.wrap_cache = None;
        self.wrap_pending = None;
        self.wrap_edit = None;
        self.scroll_col = 0;
        self.ensure_cursor_visible();
    }

    pub fn select_range(&mut self, range: Range<usize>) {
        let start = self.clamp_offset(range.start);
        let end = self.clamp_offset(range.end.max(start));
        self.anchor = Some(start);
        self.cursor = end;
        self.preferred_column = None;
        self.ensure_cursor_visible();
    }

    pub fn clear_selection(&mut self) {
        self.anchor = None;
    }

    pub fn set_status(&mut self, status: impl Into<String>) {
        self.status = status.into();
    }

    pub fn clear_status(&mut self) {
        self.status.clear();
    }

    pub fn mark_saved(&mut self) {
        self.history.mark_saved();
    }

    pub fn save(&mut self) -> Result<(), DocumentError> {
        self.document.save()?;
        self.history.mark_saved();
        self.status.clear();
        Ok(())
    }

    pub fn save_as(&mut self, path: impl AsRef<std::path::Path>) -> Result<(), DocumentError> {
        self.document.save_as(path.as_ref())?;
        self.document.set_path(path.as_ref());
        self.highlighter
            .set_language(language_for_path(path.as_ref()));
        self.history.mark_saved();
        self.status.clear();
        Ok(())
    }

    pub fn open_path(&mut self, path: impl AsRef<std::path::Path>) -> Result<(), DocumentError> {
        let document = Document::open(path)?;
        let language = document.path().and_then(language_for_path);
        self.document = document;
        self.cursor = 0;
        self.anchor = None;
        self.preferred_column = None;
        self.history.reset();
        self.history.mark_saved();
        self.highlighter.set_language(language);
        self.wrap_cache = None;
        self.wrap_pending = None;
        self.wrap_edit = None;
        self.scroll_row = 0;
        self.scroll_col = 0;
        self.status.clear();
        self.ensure_cursor_visible();
        Ok(())
    }

    pub fn view(&mut self) -> EditorView {
        let line_count = self.document.line_count();
        let cursor_line = self.cursor_line();
        let selection = self.selection();

        let (start, end, cursor_row, layout_rows) = if self.wrap {
            self.ensure_wrap_cache();
            let total_rows = self
                .wrap_cache
                .as_ref()
                .map_or(1, |cache| cache.rows.len().max(1));
            let start = self.scroll_row.min(total_rows.saturating_sub(1));
            let end = (start + self.viewport_rows).min(total_rows);
            let cursor_row = self.wrap_row_for_offset(self.cursor);
            let layout_rows = self
                .wrap_cache
                .as_ref()
                .map(|cache| cache.rows.range(start, end))
                .unwrap_or_default();
            (start, end, cursor_row, layout_rows)
        } else {
            let start = self.scroll_row.min(line_count.saturating_sub(1));
            let end = (start + self.viewport_rows).min(line_count);
            let layout_rows = (start..end)
                .map(|line| {
                    let line_start = self.document.line_start(line);
                    let line_end = self.document.line_end(line);
                    WrapRow {
                        line,
                        start: line_start,
                        end: line_end,
                        start_column: 0,
                        width: self.byte_to_visual(line, line_end),
                        continuation: false,
                    }
                })
                .collect();
            (start, end, cursor_line, layout_rows)
        };

        let cursor_visual = self.byte_to_visual(cursor_line, self.cursor);
        let mut rows = Vec::with_capacity(layout_rows.len());
        let mut cached_line = usize::MAX;
        let mut cached_text: Cow<'_, str> = Cow::Borrowed("");
        let mut cached_line_start = 0;
        let mut highlighted_line = usize::MAX;
        let mut cached_spans: Vec<Highlight<HighlightKind>> = Vec::new();

        for layout in layout_rows {
            let line = layout.line;
            if line != cached_line {
                cached_text = self.document.line_text_cow(line);
                cached_line_start = self.document.line_start(line);
                cached_line = line;
            }
            if line != highlighted_line {
                cached_spans = self
                    .highlighter
                    .highlight_line(&self.document, line)
                    .map(<[Highlight<HighlightKind>]>::to_vec)
                    .unwrap_or_default();
                highlighted_line = line;
            }

            let text = cached_text.as_ref();
            let line_start = cached_line_start;
            let local_start = layout.start.saturating_sub(line_start).min(text.len());
            let local_end = layout.end.saturating_sub(line_start).min(text.len());
            let mut runs: Vec<VisualRun> = Vec::new();
            let mut column = layout.start_column;

            for (byte, grapheme) in text[local_start..local_end].grapheme_indices(true) {
                let absolute = line_start + local_start + byte;
                let kind = token_kind_at(&cached_spans, absolute - line_start);
                let spaces = if grapheme == "\t" {
                    self.tab_width - (column % self.tab_width)
                } else {
                    0
                };
                let width = if grapheme == "\t" {
                    spaces
                } else {
                    UnicodeWidthStr::width(grapheme)
                };

                if let Some(last) = runs.last_mut()
                    && last.kind == kind
                    && last.column + last.width == column - layout.start_column
                {
                    if spaces == 0 {
                        last.text.push_str(grapheme);
                    } else {
                        last.text.extend(std::iter::repeat_n(' ', spaces));
                    }
                    last.width += width;
                } else {
                    let mut run_text = String::with_capacity(grapheme.len() + spaces);
                    if spaces == 0 {
                        run_text.push_str(grapheme);
                    } else {
                        run_text.extend(std::iter::repeat_n(' ', spaces));
                    }
                    runs.push(VisualRun {
                        text: run_text,
                        kind,
                        column: column - layout.start_column,
                        width,
                    });
                }
                column += width;
            }

            let selection = selection.as_ref().and_then(|selection| {
                if selection.start <= layout.end && selection.end > layout.start {
                    let start_column = if selection.start <= layout.start {
                        0
                    } else {
                        self.byte_to_visual_in(line_start, text, selection.start)
                            - layout.start_column
                    };
                    let end_column = if selection.end >= layout.end {
                        layout.width
                    } else {
                        self.byte_to_visual_in(line_start, text, selection.end)
                            - layout.start_column
                    };
                    (end_column > start_column)
                        .then_some((start_column.min(layout.width), end_column.min(layout.width)))
                } else {
                    None
                }
            });

            let cursor = (cursor_row >= start
                && cursor_row < end
                && cursor_row - start == rows.len())
            .then(|| {
                cursor_visual
                    .saturating_sub(layout.start_column)
                    .min(layout.width)
            });
            rows.push(VisualLine {
                number: line + 1,
                continuation: layout.continuation,
                runs,
                selection,
                cursor,
            });
        }

        EditorView {
            rows,
            scroll_row: start,
            scroll_col: self.scroll_col,
            cursor_row: if cursor_row >= start && cursor_row < end {
                Some(cursor_row - start)
            } else {
                None
            },
            cursor_col: Some(cursor_visual),
            path: self.document.path().map(ToOwned::to_owned),
            dirty: self.history.is_dirty(),
            status: self.status.clone(),
            cursor_line: cursor_line + 1,
            cursor_column: cursor_visual + 1,
            line_count,
            language: self.highlighter.language().map(|language| language.name),
            viewport_rows: self.viewport_rows,
            viewport_cols: self.viewport_cols,
            wrap: self.wrap,
            line_ending: self.document.line_ending(),
        }
    }

    pub fn wrap_pending(&self) -> bool {
        self.wrap_pending.is_some()
    }

    /// Whether a width change is large enough to build off the UI thread.
    pub fn defers_width_layout(&self) -> bool {
        self.wrap && self.document.line_count() >= DEFERRED_WRAP_LINES
    }

    pub fn poll_wrap(&mut self) -> bool {
        if !self.poll_wrap_result() {
            return false;
        }
        self.ensure_cursor_visible();
        true
    }

    fn poll_wrap_result(&mut self) -> bool {
        let Some(pending) = self.wrap_pending.take() else {
            return false;
        };
        match pending.rows.try_recv() {
            Ok(rows) => {
                let width = self.viewport_cols.max(1);
                let tab_width = self.tab_width.max(1);
                if pending.width != width
                    || pending.tab_width != tab_width
                    || pending.revision != self.document.revision()
                {
                    return false;
                }
                let rows = WrapRows::from_rows(rows);
                let last_row = rows.len().saturating_sub(1);
                self.scroll_row = rows
                    .first_row_of_line(pending.anchor_line)
                    .saturating_add(pending.anchor_offset)
                    .min(last_row);
                self.wrap_cache = Some(WrapCache {
                    revision: pending.revision,
                    width,
                    tab_width,
                    rows,
                });
                self.wrap_edit = None;
                true
            }
            Err(TryRecvError::Empty) => {
                self.wrap_pending = Some(pending);
                false
            }
            Err(TryRecvError::Disconnected) => false,
        }
    }

    fn start_wrap_job(&mut self, width: usize, tab_width: usize) {
        let Some(cache) = self.wrap_cache.as_ref() else {
            return;
        };
        let anchor_row = self.scroll_row.min(cache.rows.len().saturating_sub(1));
        let anchor_line = cache.rows.get(anchor_row).map_or(0, |row| row.line);
        let anchor_offset = anchor_row.saturating_sub(cache.rows.first_row_of_line(anchor_line));
        let revision = self.document.revision();
        let document = self.document.clone();
        let (sender, rows) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(build_wrap_rows(&document, width, tab_width));
        });
        self.wrap_pending = Some(WrapPending {
            width,
            tab_width,
            revision,
            anchor_line,
            anchor_offset,
            rows,
        });
    }

    fn ensure_wrap_cache(&mut self) {
        if !self.wrap {
            self.wrap_pending = None;
            return;
        }
        self.poll_wrap_result();
        if self.wrap_pending.is_some() {
            if self.wrap_cache.is_some() {
                return;
            }
            self.wrap_pending = None;
        }
        let revision = self.document.revision();
        let width = self.viewport_cols.max(1);
        let tab_width = self.tab_width.max(1);
        let fresh = self.wrap_cache.as_ref().is_some_and(|cache| {
            cache.revision == revision && cache.width == width && cache.tab_width == tab_width
        });
        if fresh {
            self.wrap_edit = None;
            return;
        }

        let previous = self.wrap_cache.take();
        let edit = self.wrap_edit.take();
        let line_count = self.document.line_count();

        if let (Some(mut cache), Some(edit)) = (previous, edit)
            && cache.width == width
            && cache.tab_width == tab_width
            && cache.revision.wrapping_add(1) == revision
            && cache.rows.len() > 0
            && cache.rows.get(cache.rows.len() - 1).is_some_and(|last| {
                edit.first_old_line <= last.line && edit.last_old_line <= last.line
            })
            && edit.last_new_line >= edit.first_old_line
            && edit.last_new_line < line_count
            && edit.last_new_line as isize == edit.last_old_line as isize + edit.line_delta
        {
            let prefix_end = cache.rows.first_row_of_line(edit.first_old_line);
            let suffix_start = cache.rows.first_row_of_line(edit.last_old_line + 1);
            let dirty_rows = build_wrap_rows_for_lines(
                &self.document,
                width,
                tab_width,
                edit.first_old_line,
                edit.last_new_line,
            );
            cache.rows.replace(
                prefix_end,
                suffix_start,
                dirty_rows,
                edit.line_delta,
                edit.byte_delta,
            );
            self.wrap_cache = Some(WrapCache {
                revision,
                width,
                tab_width,
                rows: cache.rows,
            });
            return;
        }

        #[cfg(test)]
        {
            self.wrap_full_rebuilds += 1;
        }
        self.wrap_cache = Some(WrapCache {
            revision,
            width,
            tab_width,
            rows: WrapRows::from_rows(build_wrap_rows(&self.document, width, tab_width)),
        });
    }

    fn wrap_row(&mut self, index: usize) -> Option<WrapRow> {
        self.ensure_wrap_cache();
        self.wrap_cache
            .as_ref()
            .and_then(|cache| cache.rows.get(index))
    }

    fn wrap_row_count(&mut self) -> usize {
        self.ensure_wrap_cache();
        self.wrap_cache
            .as_ref()
            .map_or(1, |cache| cache.rows.len().max(1))
    }

    fn wrap_row_for_offset(&mut self, offset: usize) -> usize {
        self.ensure_wrap_cache();
        self.wrap_cache
            .as_ref()
            .map_or(0, |cache| cache.rows.row_for_offset(offset, &self.document))
    }

    fn insert_text_with_kind(&mut self, text: &str, kind: EditKind) {
        if text.is_empty() {
            if let Some(range) = self.selection() {
                self.delete_range(range, kind);
            }
            return;
        }
        let before = self.cursor_state();
        let (start, end) = self
            .selection()
            .map_or((self.cursor, self.cursor), |range| (range.start, range.end));
        let removed = self.document.read(start..end);
        let old_line_count = self.document.line_count();
        let first_old_line = self.document.line_of(start);
        let last_old_line = self.document.line_of(end.min(self.document.len()));
        self.document.replace(start..end, text);
        let line_count = self.document.line_count();
        let last_new_line = self
            .document
            .line_of((start + text.len()).min(self.document.len()));
        self.wrap_edit = Some(WrapEdit {
            first_old_line,
            last_old_line,
            last_new_line,
            line_delta: line_count as isize - old_line_count as isize,
            byte_delta: text.len() as isize - (end - start) as isize,
        });
        self.cursor = start + text.len();
        self.anchor = None;
        self.preferred_column = None;
        self.highlighter.invalidate_from(first_old_line);
        self.finish_edit(
            before,
            Change {
                range: start..end,
                removed,
                inserted: text.as_bytes().to_vec(),
            },
            kind,
        );
    }

    fn delete_range(&mut self, range: Range<usize>, kind: EditKind) {
        if range.start == range.end {
            return;
        }
        let before = self.cursor_state();
        let removed = self.document.read(range.clone());
        let old_line_count = self.document.line_count();
        let first_old_line = self.document.line_of(range.start);
        let last_old_line = self.document.line_of(range.end.min(self.document.len()));
        self.document.replace(range.clone(), "");
        let line_count = self.document.line_count();
        let last_new_line = self.document.line_of(range.start.min(self.document.len()));
        self.wrap_edit = Some(WrapEdit {
            first_old_line,
            last_old_line,
            last_new_line,
            line_delta: line_count as isize - old_line_count as isize,
            byte_delta: -((range.end - range.start) as isize),
        });
        self.cursor = range.start;
        self.anchor = None;
        self.preferred_column = None;
        self.highlighter.invalidate_from(first_old_line);
        self.finish_edit(
            before,
            Change {
                range,
                removed,
                inserted: Vec::new(),
            },
            kind,
        );
    }

    fn finish_edit(&mut self, before: CursorState, change: Change, kind: EditKind) {
        let after = self.cursor_state();
        self.history.push(Transaction {
            id: 0,
            changes: vec![change],
            before,
            after,
            kind,
        });
        self.ensure_cursor_visible();
    }

    fn cursor_state(&self) -> CursorState {
        CursorState {
            offset: self.cursor,
            anchor: self.anchor,
        }
    }

    fn selection(&self) -> Option<Range<usize>> {
        let anchor = self.anchor?;
        let range = if anchor <= self.cursor {
            anchor..self.cursor
        } else {
            self.cursor..anchor
        };
        (!range.is_empty()).then_some(range)
    }

    fn set_cursor(&mut self, offset: usize, extend: bool) {
        let offset = self.clamp_offset(offset);
        if !extend {
            self.anchor = None;
        } else if self.anchor.is_none() {
            self.anchor = Some(self.cursor);
        }
        self.cursor = offset;
        self.ensure_cursor_visible();
    }

    fn clamp_offset(&self, offset: usize) -> usize {
        let mut offset = offset.min(self.document.len());
        while offset > 0 && !self.document.is_char_boundary(offset) {
            offset -= 1;
        }
        offset
    }

    fn move_vertical(&mut self, delta: isize, extend: bool) {
        if self.wrap {
            let row_index = self.wrap_row_for_offset(self.cursor);
            let total = self.wrap_row_count();
            let target =
                (row_index as isize + delta).clamp(0, total.saturating_sub(1) as isize) as usize;
            let wanted = self.preferred_column.unwrap_or_else(|| {
                self.wrap_row(row_index).map_or(0, |row| {
                    self.byte_to_visual(row.line, self.cursor)
                        .saturating_sub(row.start_column)
                })
            });
            if let Some(row) = self.wrap_row(target) {
                let offset = self
                    .visual_to_offset(row.line, row.start_column + wanted)
                    .clamp(row.start, row.end);
                self.set_cursor(offset, extend);
                self.preferred_column = Some(wanted);
            }
            return;
        }
        let line = self.cursor_line();
        let target = (line as isize + delta)
            .clamp(0, self.document.line_count().saturating_sub(1) as isize)
            as usize;
        let wanted = self
            .preferred_column
            .unwrap_or_else(|| self.byte_to_visual(line, self.cursor));
        let offset = self.visual_to_offset(target, wanted);
        self.set_cursor(offset, extend);
        self.preferred_column = Some(wanted);
    }

    fn previous_grapheme(&self, offset: usize) -> usize {
        let line = self.document.line_of(offset);
        let start = self.document.line_start(line);
        if offset <= start {
            return start.saturating_sub(1);
        }
        let text = self.document.line_text(line);
        let relative = offset - start;
        text[..relative]
            .grapheme_indices(true)
            .next_back()
            .map_or(start, |(index, _)| start + index)
    }

    fn next_grapheme(&self, offset: usize) -> usize {
        let line = self.document.line_of(offset);
        let start = self.document.line_start(line);
        let end = self.document.line_end(line);
        if offset < end {
            let text = self.document.line_text(line);
            let relative = offset - start;
            return offset + text[relative..].graphemes(true).next().map_or(0, str::len);
        }
        if end < self.document.len() {
            end + 1
        } else {
            self.document.len()
        }
    }

    fn byte_to_visual(&self, line: usize, offset: usize) -> usize {
        let start = self.document.line_start(line);
        let text = self.document.line_text_cow(line);
        self.byte_to_visual_in(start, text.as_ref(), offset)
    }

    fn byte_to_visual_in(&self, line_start: usize, text: &str, offset: usize) -> usize {
        let limit = offset.saturating_sub(line_start).min(text.len());
        let mut column = 0;
        for (byte, grapheme) in text.grapheme_indices(true) {
            if byte >= limit {
                break;
            }
            column += if grapheme == "\t" {
                self.tab_width - (column % self.tab_width)
            } else {
                UnicodeWidthStr::width(grapheme)
            };
        }
        column
    }

    fn visual_to_offset(&self, line: usize, target: usize) -> usize {
        let start = self.document.line_start(line);
        let text = self.document.line_text_cow(line);
        let text = text.as_ref();
        let mut column = 0;
        for (byte, grapheme) in text.grapheme_indices(true) {
            if target <= column {
                return start + byte;
            }
            let width = if grapheme == "\t" {
                self.tab_width - (column % self.tab_width)
            } else {
                UnicodeWidthStr::width(grapheme)
            };
            if target < column + width {
                return start + byte;
            }
            column += width;
        }
        start + text.len()
    }

    fn ensure_cursor_visible(&mut self) {
        if self.wrap {
            let row = self.wrap_row_for_offset(self.cursor);
            let total = self.wrap_row_count();
            if row < self.scroll_row {
                self.scroll_row = row;
            } else if row >= self.scroll_row + self.viewport_rows {
                self.scroll_row = row + 1 - self.viewport_rows;
            }
            self.scroll_col = 0;
            self.scroll_row = self.scroll_row.min(total.saturating_sub(1));
            return;
        }

        let line = self.cursor_line();
        if line < self.scroll_row {
            self.scroll_row = line;
        } else if line >= self.scroll_row + self.viewport_rows {
            self.scroll_row = line + 1 - self.viewport_rows;
        }

        let column = self.byte_to_visual(line, self.cursor);
        if column < self.scroll_col {
            self.scroll_col = column;
        } else if column >= self.scroll_col + self.viewport_cols {
            self.scroll_col = column + 1 - self.viewport_cols;
        }

        let last_line = self.document.line_count().saturating_sub(1);
        self.scroll_row = self.scroll_row.min(last_line);
    }
}

fn build_wrap_rows(document: &Document, width: usize, tab_width: usize) -> Vec<WrapRow> {
    let line_count = document.line_count();
    if line_count == 0 {
        return Vec::new();
    }

    // Width changes force every line to be measured again. Split that work
    // across cores so a large document does not monopolize the UI thread for
    // one long serial pass. Small documents stay on the cheap serial path.
    let workers = std::thread::available_parallelism()
        .map_or(1, |parallelism| parallelism.get())
        .min(8);
    if workers < 2 || line_count < 4096 {
        return build_wrap_rows_for_lines(document, width, tab_width, 0, line_count - 1);
    }

    let chunk = line_count.div_ceil(workers);
    let ranges: Vec<(usize, usize)> = (0..workers)
        .filter_map(|index| {
            let first = index * chunk;
            (first < line_count).then(|| (first, (first + chunk).min(line_count) - 1))
        })
        .collect();

    let parts: Vec<Vec<WrapRow>> = std::thread::scope(|scope| {
        let handles: Vec<_> = ranges
            .into_iter()
            .map(|(first, last)| {
                scope.spawn(move || {
                    build_wrap_rows_for_lines(document, width, tab_width, first, last)
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("wrap worker panicked"))
            .collect()
    });
    parts.into_iter().flatten().collect()
}

fn build_wrap_rows_for_lines(
    document: &Document,
    width: usize,
    tab_width: usize,
    first_line: usize,
    last_line: usize,
) -> Vec<WrapRow> {
    let width = width.max(1);
    let tab_width = tab_width.max(1);
    let mut rows = Vec::new();
    let mut graphemes = Vec::new();

    for line in first_line..=last_line {
        append_wrap_line(document, line, width, tab_width, &mut rows, &mut graphemes);
    }

    rows
}

fn append_wrap_line(
    document: &Document,
    line: usize,
    width: usize,
    tab_width: usize,
    rows: &mut Vec<WrapRow>,
    graphemes: &mut Vec<(usize, usize)>,
) {
    let line_start = document.line_start(line);
    let text = document.line_text_cow(line);
    let text = text.as_ref();
    if text.is_empty() {
        rows.push(WrapRow {
            line,
            start: line_start,
            end: line_start,
            start_column: 0,
            width: 0,
            continuation: false,
        });
        return;
    }

    graphemes.clear();
    graphemes.extend(
        text.grapheme_indices(true)
            .map(|(start, grapheme)| (start, start + grapheme.len())),
    );
    let mut segment_start_index = 0;
    let mut segment_start_byte = line_start;
    let mut segment_start_column = 0;
    let mut column = 0;
    let mut index = 0;
    let mut last_break: Option<(usize, usize, usize)> = None;
    let mut continuation = false;

    while index < graphemes.len() {
        let (byte, end_byte) = graphemes[index];
        let grapheme = &text[byte..end_byte];
        let glyph_width = if grapheme == "\t" {
            tab_width - (column % tab_width)
        } else {
            UnicodeWidthStr::width(grapheme)
        };

        if column.saturating_sub(segment_start_column) + glyph_width > width
            && index > segment_start_index
        {
            let (end_index, end_byte, end_column) = last_break
                .filter(|(break_index, _, _)| *break_index > segment_start_index)
                .unwrap_or((index, line_start + byte, column));
            rows.push(WrapRow {
                line,
                start: segment_start_byte,
                end: end_byte,
                start_column: segment_start_column,
                width: end_column.saturating_sub(segment_start_column),
                continuation,
            });
            continuation = true;
            segment_start_index = end_index;
            segment_start_byte = end_byte;
            segment_start_column = end_column;
            column = end_column;
            index = end_index;
            last_break = None;
            continue;
        }

        column += glyph_width;
        index += 1;
        if grapheme.chars().all(char::is_whitespace) {
            last_break = Some((index, line_start + byte + grapheme.len(), column));
        }
    }

    rows.push(WrapRow {
        line,
        start: segment_start_byte,
        end: line_start + text.len(),
        start_column: segment_start_column,
        width: column.saturating_sub(segment_start_column),
        continuation,
    });
}

fn token_kind_at(spans: &[Highlight<HighlightKind>], byte: usize) -> TokenKind {
    let index = spans.partition_point(|span| span.start <= byte);
    if index == 0 {
        TokenKind::Plain
    } else {
        TokenKind::from_lsh(spans[index - 1].kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor(text: &str) -> Editor {
        Editor::new(Document::from_text(text))
    }

    #[test]
    fn typing_and_backspace_are_undoable() {
        let mut editor = editor("");
        editor.insert_text("abc");
        editor.backspace();
        assert_eq!(editor.document().text(), "ab");
        editor.undo();
        assert_eq!(editor.document().text(), "abc");
        editor.undo();
        assert_eq!(editor.document().text(), "");
        editor.redo();
        assert_eq!(editor.document().text(), "abc");
        editor.redo();
        assert_eq!(editor.document().text(), "ab");
    }

    #[test]
    fn adjacent_typing_is_one_transaction() {
        let mut editor = editor("");
        editor.insert_text("a");
        editor.insert_text("b");
        editor.insert_text("c");
        assert_eq!(editor.document().text(), "abc");
        editor.undo();
        assert_eq!(editor.document().text(), "");
    }

    #[test]
    fn selection_is_replaced_and_restored_by_undo() {
        let mut editor = editor("hello world");
        editor.click(0, 6, false);
        editor.click(0, 11, true);
        assert_eq!(editor.selected_text().as_deref(), Some("world"));
        editor.insert_text("there");
        assert_eq!(editor.document().text(), "hello there");
        editor.undo();
        assert_eq!(editor.document().text(), "hello world");
        assert_eq!(editor.selected_text().as_deref(), Some("world"));
    }

    #[test]
    fn vertical_movement_preserves_visual_column() {
        let mut editor = editor("abcdef\nxy\nabcdef");
        editor.click(0, 4, false);
        editor.move_down(false);
        assert_eq!(editor.cursor_line(), 1);
        assert_eq!(editor.cursor_column(), 2);
        editor.move_down(false);
        assert_eq!(editor.cursor_line(), 2);
        assert_eq!(editor.cursor_column(), 4);
    }

    #[test]
    fn tab_and_wide_characters_have_visual_widths() {
        let mut editor = editor("\t中a");
        editor.click(0, 4, false);
        assert_eq!(editor.cursor_column(), 4);
        editor.move_right(false);
        assert_eq!(editor.cursor_column(), 6);
        editor.move_right(false);
        assert_eq!(editor.cursor_column(), 7);
    }

    #[test]
    fn saving_sets_clean_and_further_edits_dirty_it() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("doc.txt");
        let mut editor = editor("one");
        editor.insert_text("!");
        editor.save_as(&path).expect("save");
        assert!(!editor.dirty());
        editor.insert_text("?");
        assert!(editor.dirty());
    }

    #[test]
    fn undo_after_save_removes_only_the_post_save_edit() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("doc.txt");
        let mut editor = editor("one");
        editor.save_as(&path).expect("save");
        editor.move_document_end(false);
        editor.insert_text("!");
        assert_eq!(editor.document().text(), "one!");
        assert!(editor.dirty());
        editor.undo();
        assert_eq!(editor.document().text(), "one");
        assert!(!editor.dirty());
    }

    #[test]
    fn undoing_a_new_branch_returns_to_the_saved_content() {
        let mut editor = editor("");
        editor.insert_text("a");
        editor.undo();
        assert!(!editor.dirty());
        editor.insert_text("b");
        assert!(editor.dirty());
        editor.undo();
        assert_eq!(editor.document().text(), "");
        assert!(!editor.dirty());
    }

    #[test]
    fn redo_after_undoing_a_saved_edit_is_dirty_again() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("doc.txt");
        let mut editor = editor("");
        editor.insert_text("a");
        editor.save_as(&path).expect("save");
        editor.undo();
        assert!(editor.dirty());
        editor.redo();
        assert!(!editor.dirty());
        assert_eq!(editor.document().text(), "a");
    }

    #[test]
    fn large_width_changes_finish_off_the_ui_thread() {
        let text = (0..5000)
            .map(|line| format!("line {line} with words"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut editor = Editor::new(Document::from_text(&text));
        editor.set_viewport(20, 13);
        editor.set_wrap(true);
        assert!(!editor.wrap_pending());

        editor.set_viewport(20, 7);
        assert!(editor.wrap_pending());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while editor.wrap_pending() && std::time::Instant::now() < deadline {
            editor.poll_wrap();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(!editor.wrap_pending());

        let expected = build_wrap_rows(&Document::from_text(&text), 7, DEFAULT_TAB_WIDTH);
        let cache = editor.wrap_cache.as_ref().expect("new layout");
        assert_eq!(cache.rows.len(), expected.len());
        for (index, row) in expected.iter().enumerate() {
            assert_eq!(cache.rows.get(index).as_ref(), Some(row));
        }
    }

    #[test]
    fn parallel_wrap_matches_the_serial_layout() {
        let text = (0..5000)
            .map(|line| format!("line {line} with words"))
            .collect::<Vec<_>>()
            .join("\n");
        let document = Document::from_text(&text);
        let last = document.line_count() - 1;
        assert_eq!(
            build_wrap_rows(&document, 13, DEFAULT_TAB_WIDTH),
            build_wrap_rows_for_lines(&document, 13, DEFAULT_TAB_WIDTH, 0, last)
        );
    }

    #[test]
    fn wrap_segments_survive_many_edits_across_chunks() {
        let text = (0..700)
            .map(|line| format!("line {line} with enough words to wrap twice"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut editor = editor(&text);
        editor.set_viewport(10, 14);
        editor.set_wrap(true);
        editor.ensure_wrap_cache();
        assert!(
            editor
                .wrap_cache
                .as_ref()
                .expect("cache")
                .rows
                .segment_count()
                > 1
        );
        let full_rebuilds = editor.wrap_full_rebuilds;

        let mut state = 0xdead_beef_u64;
        for _ in 0..250 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let start = (state as usize) % (editor.document.len() + 1);
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let end = start + ((state as usize) % (editor.document.len() + 1 - start));
            editor.select_range(start..end);
            if state.is_multiple_of(2) {
                editor.backspace();
            } else {
                editor.insert_text("\nnew line\n");
            }

            editor.ensure_wrap_cache();
            let cache = editor.wrap_cache.as_ref().expect("cache");
            assert_eq!(
                cache.rows.to_vec(),
                build_wrap_rows(&editor.document, 14, DEFAULT_TAB_WIDTH)
            );
        }
        assert_eq!(editor.wrap_full_rebuilds, full_rebuilds);
    }

    #[test]
    fn wrapped_edits_reuse_the_previous_layout() {
        let mut editor = editor(
            "alpha beta gamma\ndelta epsilon\nzeta eta theta\niota kappa lambda\n             mu nu xi omicron\npi rho sigma\ntau upsilon phi\nchi psi omega",
        );
        editor.set_viewport(6, 11);
        editor.set_wrap(true);
        editor.ensure_wrap_cache();

        let full_rebuilds = editor.wrap_full_rebuilds;
        let mut state = 0x1234_5678_u64;
        let inserts = ["x", " ", "\n", "ab", "longword", "\n\n", "\t"];

        for _ in 0..400 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let len = editor.document().len();
            if state.is_multiple_of(3) && len > 1 {
                let start = (state as usize) % len;
                let end = start + ((state as usize / 7) % (len - start + 1));
                editor.select_range(start..end);
                editor.backspace();
            } else {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                let offset = (state as usize) % (editor.document().len() + 1);
                editor.select_range(offset..offset);
                editor.insert_text(inserts[(state as usize / 5) % inserts.len()]);
            }

            editor.ensure_wrap_cache();
            let cache = editor.wrap_cache.as_ref().expect("wrap cache");
            let expected = build_wrap_rows(&editor.document, 11, DEFAULT_TAB_WIDTH);
            assert_eq!(cache.rows.to_vec(), expected);
            assert_eq!(editor.wrap_full_rebuilds, full_rebuilds);
            let expected_rows = WrapRows::from_rows(expected);
            for offset in [0, editor.document.len() / 2, editor.document.len()] {
                assert_eq!(
                    cache.rows.row_for_offset(offset, &editor.document),
                    expected_rows.row_for_offset(offset, &editor.document)
                );
            }
            let _ = editor.view();
        }
    }

    #[test]
    fn soft_wrap_builds_continuation_rows_and_click_targets() {
        let mut editor = editor("abcdefghijklmnopqrstuvwxyz\nsecond\n");
        editor.set_viewport(20, 12);
        editor.set_wrap(true);
        let view = editor.view();
        assert!(view.rows.len() >= 3);
        assert!(view.rows[1].continuation);
        assert_eq!(view.rows[1].number, 1);

        editor.click(0, 5, false);
        assert_eq!(editor.cursor_offset(), 5);
        editor.click(1, 0, false);
        assert_eq!(editor.cursor_offset(), 12);
        editor.move_end(false);
        assert_eq!(editor.cursor_offset(), 24);
    }

    #[test]
    fn wrapped_rows_respect_width_and_do_not_show_leading_space() {
        let mut editor = editor("one two three four\n");
        editor.set_viewport(20, 7);
        editor.set_wrap(true);
        let view = editor.view();
        for row in &view.rows {
            let text: String = row.runs.iter().map(|run| run.text.as_str()).collect();
            let width: usize = row.runs.iter().map(|run| run.width).sum();
            assert!(width <= 7, "row {text:?} is wider than the viewport");
            assert!(
                !text.starts_with(' ') || row.number == 1 && text.trim().is_empty(),
                "row starts with a stray space: {text:?}"
            );
        }
    }

    #[test]
    fn empty_replacement_deletes_the_selection() {
        let mut editor = editor("abc");
        editor.select_range(0..3);
        editor.insert_text("");
        assert_eq!(editor.document().text(), "");
        editor.undo();
        assert_eq!(editor.document().text(), "abc");
    }

    #[test]
    fn click_uses_the_current_scroll_origin() {
        let mut editor = editor("one\ntwo\nthree\nfour\nfive\n");
        editor.set_viewport(2, 20);
        editor.move_document_end(false);
        let view = editor.view();
        assert!(view.scroll_row > 0);
        let first_visible_line = view.scroll_row;
        editor.click(0, 0, false);
        assert_eq!(editor.cursor_line(), first_visible_line);
    }

    #[test]
    fn manual_scroll_is_not_undone_by_rendering() {
        let mut editor = editor("one\ntwo\nthree\nfour\n");
        editor.set_viewport(2, 20);
        editor.click(0, 0, false);
        let before = editor.view().scroll_row;
        editor.scroll(0, 1);
        assert_eq!(editor.view().scroll_row, before + 1);
    }

    #[test]
    fn cut_without_selection_removes_the_line() {
        let mut editor = editor("one\ntwo\n");
        editor.click(0, 1, false);
        let text = editor.cut().expect("cut");
        assert_eq!(text, "one\n");
        assert_eq!(editor.document().text(), "two\n");
        editor.undo();
        assert_eq!(editor.document().text(), "one\ntwo\n");
    }
}
