//! Immutable view data consumed by a frontend.

use std::path::PathBuf;

use lsh::definitions::HighlightKind;

/// Frontend-neutral token categories.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenKind {
    Plain,
    Comment,
    String,
    Number,
    Keyword,
    Function,
    Type,
    Variable,
    Constant,
    Markup,
}

impl TokenKind {
    pub(crate) fn from_lsh(kind: HighlightKind) -> Self {
        match kind {
            HighlightKind::Other => Self::Plain,
            HighlightKind::Comment => Self::Comment,
            HighlightKind::Method => Self::Function,
            HighlightKind::String => Self::String,
            HighlightKind::Variable => Self::Variable,
            HighlightKind::ConstantLanguage => Self::Constant,
            HighlightKind::ConstantNumeric => Self::Number,
            HighlightKind::KeywordControl
            | HighlightKind::KeywordOther
            | HighlightKind::KeywordPreprocessor
            | HighlightKind::StorageAnnotation => Self::Keyword,
            HighlightKind::StorageType => Self::Type,
            HighlightKind::MarkupBold
            | HighlightKind::MarkupChanged
            | HighlightKind::MarkupDeleted
            | HighlightKind::MarkupHeading
            | HighlightKind::MarkupInserted
            | HighlightKind::MarkupItalic
            | HighlightKind::MarkupLink
            | HighlightKind::MarkupList
            | HighlightKind::MarkupStrikethrough => Self::Markup,
            HighlightKind::MetaHeader => Self::Comment,
        }
    }
}

/// One styled run inside a visible line.
#[derive(Clone, Debug)]
pub struct VisualRun {
    pub text: String,
    pub kind: TokenKind,
    /// Visual column relative to the beginning of the logical line.
    pub column: usize,
    /// Number of terminal cells occupied by `text`.
    pub width: usize,
}

/// One visible logical line.
#[derive(Clone, Debug)]
pub struct VisualLine {
    /// One-based logical line number.
    pub number: usize,
    /// Whether this row continues a soft-wrapped logical line.
    pub continuation: bool,
    pub runs: Vec<VisualRun>,
    /// Selection range in visual columns, or `None`.
    pub selection: Option<(usize, usize)>,
    /// Cursor column if the caret is on this line.
    pub cursor: Option<usize>,
}

/// Everything a frontend needs to draw one frame.
#[derive(Clone, Debug)]
pub struct EditorView {
    pub rows: Vec<VisualLine>,
    pub scroll_row: usize,
    pub scroll_col: usize,
    pub cursor_row: Option<usize>,
    pub cursor_col: Option<usize>,
    pub path: Option<PathBuf>,
    pub dirty: bool,
    pub status: String,
    /// One-based cursor line for the status bar.
    pub cursor_line: usize,
    /// One-based cursor column for the status bar.
    pub cursor_column: usize,
    pub line_count: usize,
    pub language: Option<&'static str>,
    pub viewport_rows: usize,
    pub viewport_cols: usize,
    pub wrap: bool,
    pub line_ending: crate::document::LineEnding,
}

impl EditorView {
    pub fn title(&self) -> String {
        let name = self
            .path
            .as_ref()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("Untitled");
        if self.dirty {
            format!("{name} *")
        } else {
            name.to_string()
        }
    }
}
