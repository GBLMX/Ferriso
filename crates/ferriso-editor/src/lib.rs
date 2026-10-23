//! GUI-independent text editor core for Ferriso.
//!
//! The editor owns a UTF-8 gap buffer, cursor and selection, a transactional
//! undo stack, line layout helpers, and incremental LSH highlighting.
//! It has no dependency on a windowing toolkit; the iced application in
//! `apps/editor` only translates input and draws [`EditorView`] snapshots.

mod document;
mod editor;
mod gap;
mod highlight;
mod segmented;
mod view;

pub use document::{Document, DocumentError, LineEnding};
pub use editor::Editor;
pub use view::{EditorView, TokenKind, VisualLine, VisualRun};
