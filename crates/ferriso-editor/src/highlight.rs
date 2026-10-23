//! Line-oriented syntax highlighting through the vendored LSH bytecode runtime.

use std::collections::BTreeMap;
use std::path::Path;

use lsh::definitions::{ASSEMBLY, CHARSETS, FILE_ASSOCIATIONS, HighlightKind, STRINGS};
use lsh::runtime::{Highlight, Language, Runtime, RuntimeState};

use crate::document::Document;

const CHECKPOINT_INTERVAL: usize = 64;

/// Returns the LSH language for a path using the generated file associations.
pub fn language_for_path(path: &Path) -> Option<&'static Language> {
    let name = path.file_name()?.to_str()?;
    FILE_ASSOCIATIONS
        .iter()
        .find_map(|(pattern, language)| association_matches(pattern, name).then_some(*language))
}

fn association_matches(pattern: &str, name: &str) -> bool {
    if let Some(extension) = pattern.strip_prefix("**/*.") {
        return name
            .rsplit_once('.')
            .is_some_and(|(_, actual)| actual.eq_ignore_ascii_case(extension));
    }
    if let Some(exact) = pattern.strip_prefix("**/") {
        return name == exact;
    }
    name == pattern
}

/// Incremental highlighter for one language.
pub struct Highlighter {
    language: Option<&'static Language>,
    runtime: Runtime<'static, 'static, 'static>,
    next_line: usize,
    checkpoints: Vec<(usize, RuntimeState)>,
    cache: BTreeMap<usize, Vec<Highlight<HighlightKind>>>,
}

impl Highlighter {
    /// Create a highlighter for a language, or a disabled one when `None`.
    pub fn new(language: Option<&'static Language>) -> Self {
        let mut highlighter = Self {
            language,
            runtime: Self::make_runtime(language),
            next_line: 0,
            checkpoints: Vec::new(),
            cache: BTreeMap::new(),
        };
        highlighter.reset_runtime();
        highlighter
    }

    /// Select another language and discard cached state.
    pub fn set_language(&mut self, language: Option<&'static Language>) {
        self.language = language;
        self.reset_runtime();
    }

    pub fn language(&self) -> Option<&'static Language> {
        self.language
    }

    /// Drop cached spans and checkpoints at or after `line`.
    pub fn invalidate_from(&mut self, line: usize) {
        let _ = self.cache.split_off(&line);
        let keep = self
            .checkpoints
            .partition_point(|(cached, _)| *cached < line);
        self.checkpoints.truncate(keep);

        if let Some((checkpoint_line, state)) = self.checkpoints.last() {
            self.runtime.restore(state);
            self.next_line = *checkpoint_line;
        } else {
            self.reset_runtime();
        }
    }

    /// Highlight one logical line, computing missing state from the nearest
    /// checkpoint. The returned spans use byte offsets relative to the line.
    pub fn highlight_line(
        &mut self,
        document: &Document,
        line: usize,
    ) -> Option<&[Highlight<HighlightKind>]> {
        if self.language.is_none() || line >= document.line_count() {
            return None;
        }

        if !self.cache.contains_key(&line) {
            if line < self.next_line {
                self.restore_for_line(line);
            }
            while self.next_line <= line {
                if self.next_line.is_multiple_of(CHECKPOINT_INTERVAL)
                    && self
                        .checkpoints
                        .last()
                        .is_none_or(|(checkpoint, _)| *checkpoint != self.next_line)
                {
                    self.checkpoints
                        .push((self.next_line, self.runtime.snapshot()));
                }

                let text = document.line_text(self.next_line);
                let mut spans = Vec::new();
                self.runtime.parse_next_line(text.as_bytes(), &mut spans);
                self.cache.insert(self.next_line, spans);
                self.next_line += 1;
            }
        }

        self.cache.get(&line).map(Vec::as_slice)
    }

    fn restore_for_line(&mut self, line: usize) {
        let index = self
            .checkpoints
            .partition_point(|(checkpoint, _)| *checkpoint <= line)
            .saturating_sub(1);
        if let Some((checkpoint_line, state)) = self.checkpoints.get(index) {
            self.runtime.restore(state);
            self.next_line = *checkpoint_line;
        } else {
            self.reset_runtime();
        }
    }

    fn reset_runtime(&mut self) {
        self.runtime = Self::make_runtime(self.language);
        self.next_line = 0;
        self.checkpoints.clear();
        self.cache.clear();
    }

    fn make_runtime(language: Option<&'static Language>) -> Runtime<'static, 'static, 'static> {
        let entrypoint = language.map_or(0, |language| language.entrypoint);
        Runtime::new(&ASSEMBLY, &STRINGS, &CHARSETS, entrypoint)
    }
}

impl Default for Highlighter {
    fn default() -> Self {
        Self::new(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_file_association_is_found() {
        let language = language_for_path(Path::new("/tmp/main.rs")).expect("rust language");
        assert_eq!(language.id, "rust");
        assert!(language_for_path(Path::new("/tmp/main.unknown")).is_none());
    }

    #[test]
    fn rust_comments_are_highlighted() {
        let language = language_for_path(Path::new("sample.rs")).expect("rust");
        let mut highlighter = Highlighter::new(Some(language));
        let document = Document::from_text("// comment\nfn main() {}\n");

        let spans = highlighter
            .highlight_line(&document, 0)
            .expect("highlights");
        assert!(spans.iter().any(|span| span.kind == HighlightKind::Comment));
    }

    #[test]
    fn invalidation_reparses_changed_lines() {
        let language = language_for_path(Path::new("sample.rs")).expect("rust");
        let mut highlighter = Highlighter::new(Some(language));
        let mut document = Document::from_text("let x = 1;\n");
        assert!(highlighter.highlight_line(&document, 0).is_some());

        document.replace(0..0, "// prefix\n");
        highlighter.invalidate_from(0);
        let spans = highlighter
            .highlight_line(&document, 0)
            .expect("highlights after edit");
        assert!(spans.iter().any(|span| span.kind == HighlightKind::Comment));
    }
}
