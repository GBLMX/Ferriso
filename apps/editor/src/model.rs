//! Actor-owned editor state.
//!
//! This is the only module that mutates tabs, search, and save state. The iced
//! shell renders [`EditorSnapshot`] and sends [`EditorMessage`]; it never
//! borrows an [`Editor`].

use std::{
    ops::Range,
    path::{Path, PathBuf},
    sync::Arc,
};

use ferriso_core::{App, Command};
use ferriso_editor::{Editor, EditorView};
use ferriso_platform::{
    Clipboard, FileDialog, SystemClipboard, SystemFileDialog, pick_file_async, save_file_async,
};

struct Tab {
    editor: Editor,
}

#[derive(Clone, Debug)]
pub struct TabInfo {
    pub name: String,
    pub dirty: bool,
}

#[derive(Clone, Debug)]
pub struct SearchSnapshot {
    pub open: bool,
    pub replace: bool,
    pub query: String,
    pub replacement: String,
    pub case_sensitive: bool,
    pub matches: Vec<Range<usize>>,
    pub current: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusTarget {
    Query,
    Replacement,
}

#[derive(Clone, Copy, Debug)]
pub struct FocusRequest {
    pub target: FocusTarget,
    pub id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Modal {
    CloseTab(usize),
    Quit,
}

/// The dialog result to apply after the blocking picker returns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogAction {
    Open,
    Save { index: usize, close_after: bool },
    SaveAs { index: usize },
    SaveAll { index: usize },
}

#[derive(Clone, Debug)]
pub struct EditorSnapshot {
    pub tabs: Vec<TabInfo>,
    pub active: usize,
    pub view: EditorView,
    pub search: SearchSnapshot,
    pub modal: Option<Modal>,
    pub dirty_count: usize,
    pub focus: Option<FocusRequest>,
    pub wrap_pending: bool,
    pub defer_resize: bool,
}

impl EditorSnapshot {
    pub fn title(&self) -> String {
        format!("{} · Ferriso Editor", self.view.title())
    }

    pub fn active_name(&self) -> &str {
        self.tabs
            .get(self.active)
            .map_or("Untitled", |tab| tab.name.as_str())
    }
}

#[derive(Clone, Debug)]
pub enum EditorMessage {
    New,
    Open,
    Save,
    SaveAs,
    SelectTab(usize),
    NextTab,
    PreviousTab,
    CloseTab(usize),
    CloseActiveTab,
    Undo,
    Redo,
    Copy,
    Cut,
    Paste,
    SelectAll,
    Insert(String),
    Backspace,
    DeleteForward,
    Enter,
    Tab,
    MoveLeft(bool),
    MoveRight(bool),
    MoveUp(bool),
    MoveDown(bool),
    MoveHome(bool),
    MoveEnd(bool),
    MoveDocumentStart(bool),
    MoveDocumentEnd(bool),
    PageUp(bool),
    PageDown(bool),
    Pointer {
        row: usize,
        column: usize,
        extend: bool,
    },
    Scroll {
        columns: isize,
        rows: isize,
    },
    Viewport {
        rows: usize,
        columns: usize,
    },
    PollWrap,
    ToggleWrap,
    Find,
    Replace,
    CloseSearch,
    SearchQuery(String),
    Replacement(String),
    ToggleCase,
    FindNext,
    FindPrevious,
    FocusReplacement,
    ReplaceCurrent,
    ReplaceAll,
    ModalSave,
    ModalDiscard,
    ModalCancel,
    Escape,
    Quit,
    CloseRequested,
    DialogResult {
        action: DialogAction,
        result: Result<Option<PathBuf>, String>,
    },
}

#[derive(Default)]
struct SearchState {
    open: bool,
    replace: bool,
    query: String,
    replacement: String,
    case_sensitive: bool,
    matches: Vec<Range<usize>>,
    current: Option<usize>,
    focus_replacement: bool,
    cached: Option<(u64, String, bool)>,
}

/// The application state owned by the Ferriso actor.
pub struct EditorModel {
    tabs: Vec<Tab>,
    active: usize,
    viewport: (usize, usize),
    search: SearchState,
    modal: Option<Modal>,
    focus: Option<FocusRequest>,
    next_focus: u64,
    quit: bool,
    snapshot: EditorSnapshot,
    clipboard: Option<SystemClipboard>,
    dialog: Arc<dyn FileDialog>,
    pending_dialog: Option<DialogAction>,
}

impl EditorModel {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self::with_dialog(path, Arc::new(SystemFileDialog::new()))
    }

    /// Build a model with an injectable dialog backend.
    #[must_use]
    pub fn with_dialog(path: Option<PathBuf>, dialog: Arc<dyn FileDialog>) -> Self {
        let mut editor = match path {
            Some(path) => match Editor::open(&path) {
                Ok(editor) => editor,
                Err(error) => {
                    let mut editor = Editor::empty();
                    editor.set_status(format!("{error}"));
                    editor
                }
            },
            None => Editor::empty(),
        };
        editor.set_viewport(34, 120);
        let view = editor.view();
        let mut model = Self {
            tabs: vec![Tab { editor }],
            active: 0,
            viewport: (34, 120),
            search: SearchState::default(),
            modal: None,
            focus: None,
            next_focus: 1,
            quit: false,
            snapshot: EditorSnapshot {
                tabs: Vec::new(),
                active: 0,
                view,
                search: SearchSnapshot {
                    open: false,
                    replace: false,
                    query: String::new(),
                    replacement: String::new(),
                    case_sensitive: false,
                    matches: Vec::new(),
                    current: None,
                },
                modal: None,
                dirty_count: 0,
                focus: None,
                wrap_pending: false,
                defer_resize: false,
            },
            clipboard: SystemClipboard::new().ok(),
            dialog,
            pending_dialog: None,
        };
        model.refresh();
        model
    }

    pub fn from_args() -> Self {
        Self::new(std::env::args_os().nth(1).map(PathBuf::from))
    }

    fn refresh(&mut self) {
        let view = self.tabs[self.active].editor.view();
        if self.search.open {
            self.sync_search();
        }
        let tabs = self
            .tabs
            .iter()
            .map(|tab| TabInfo {
                name: tab_name(tab),
                dirty: tab.editor.dirty(),
            })
            .collect();
        let dirty_count = self.tabs.iter().filter(|tab| tab.editor.dirty()).count();
        let wrap_pending = self.tabs.iter().any(|tab| tab.editor.wrap_pending());
        let defer_resize = self.tabs[self.active].editor.defers_width_layout();
        self.snapshot = EditorSnapshot {
            tabs,
            active: self.active,
            view,
            search: SearchSnapshot {
                open: self.search.open,
                replace: self.search.replace,
                query: self.search.query.clone(),
                replacement: self.search.replacement.clone(),
                case_sensitive: self.search.case_sensitive,
                matches: self.search.matches.clone(),
                current: self.search.current,
            },
            modal: self.modal,
            dirty_count,
            focus: self.focus,
            wrap_pending,
            defer_resize,
        };
    }

    fn active_editor(&self) -> &Editor {
        &self.tabs[self.active].editor
    }

    fn active_editor_mut(&mut self) -> &mut Editor {
        &mut self.tabs[self.active].editor
    }

    fn dirty_count(&self) -> usize {
        self.tabs.iter().filter(|tab| tab.editor.dirty()).count()
    }

    fn request_focus(&mut self, target: FocusTarget) {
        self.focus = Some(FocusRequest {
            target,
            id: self.next_focus,
        });
        self.next_focus = self.next_focus.wrapping_add(1);
    }

    fn new_tab(&mut self) {
        let mut editor = Editor::empty();
        editor.set_viewport(self.viewport.0, self.viewport.1);
        self.tabs.push(Tab { editor });
        self.active = self.tabs.len() - 1;
        self.search.current = None;
        self.search.cached = None;
    }

    fn select_tab(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active = index;
            self.search.current = None;
            self.search.cached = None;
        }
    }

    fn cycle_tab(&mut self, delta: isize) {
        if self.tabs.is_empty() {
            return;
        }
        let len = self.tabs.len() as isize;
        self.active = ((self.active as isize + delta).rem_euclid(len)) as usize;
        self.search.current = None;
        self.search.cached = None;
    }

    fn close_tab(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }
        if self.tabs.len() == 1 {
            let mut editor = Editor::empty();
            editor.set_viewport(self.viewport.0, self.viewport.1);
            self.tabs[0].editor = editor;
            self.active = 0;
        } else {
            self.tabs.remove(index);
            if self.active >= self.tabs.len() {
                self.active = self.tabs.len() - 1;
            } else if index < self.active {
                self.active -= 1;
            }
        }
        self.search.current = None;
        self.search.cached = None;
    }

    fn open_dialog(&mut self) -> Command<EditorMessage> {
        self.start_dialog(DialogAction::Open)
    }

    fn open_file(&mut self, path: PathBuf) {
        if let Some(index) = self
            .tabs
            .iter()
            .position(|tab| tab.editor.path() == Some(path.as_path()))
        {
            self.select_tab(index);
            return;
        }
        match Editor::open(&path) {
            Ok(mut editor) => {
                editor.set_viewport(self.viewport.0, self.viewport.1);
                let reusable = self.tabs[self.active].editor.path().is_none()
                    && !self.tabs[self.active].editor.dirty()
                    && self.tabs[self.active].editor.document().text().is_empty();
                if reusable {
                    self.tabs[self.active].editor = editor;
                } else {
                    self.tabs.push(Tab { editor });
                    self.active = self.tabs.len() - 1;
                }
                self.search.current = None;
                self.search.cached = None;
            }
            Err(error) => self.active_editor_mut().set_status(format!("{error}")),
        }
    }

    fn start_dialog(&mut self, action: DialogAction) -> Command<EditorMessage> {
        if self.pending_dialog.is_some() {
            return Command::none();
        }
        self.pending_dialog = Some(action);
        let dialog = Arc::clone(&self.dialog);
        let open = matches!(action, DialogAction::Open);

        Command::perform(
            async move {
                let result = if open {
                    pick_file_async(dialog).await
                } else {
                    save_file_async(dialog).await
                };
                result.map_err(|error| error.to_string())
            },
            move |result| EditorMessage::DialogResult { action, result },
        )
    }

    fn save_current(&mut self) -> Command<EditorMessage> {
        let index = self.active;
        if index >= self.tabs.len() {
            return Command::none();
        }
        if self.tabs[index].editor.path().is_some() {
            self.save_tab_existing(index);
            return Command::none();
        }
        self.start_save_dialog(
            index,
            DialogAction::Save {
                index,
                close_after: false,
            },
        )
    }

    fn save_as_dialog(&mut self) -> Command<EditorMessage> {
        let index = self.active;
        if index >= self.tabs.len() {
            return Command::none();
        }
        self.start_save_dialog(index, DialogAction::SaveAs { index })
    }

    fn save_all(&mut self) -> Command<EditorMessage> {
        let indices: Vec<usize> = self
            .tabs
            .iter()
            .enumerate()
            .filter(|(_, tab)| tab.editor.dirty())
            .map(|(index, _)| index)
            .collect();
        for index in indices {
            if self.tabs[index].editor.path().is_none() {
                return self.start_save_dialog(index, DialogAction::SaveAll { index });
            }
            if !self.save_tab_existing(index) {
                return Command::none();
            }
        }
        self.modal = None;
        self.quit = true;
        Command::none()
    }

    fn start_save_dialog(&mut self, index: usize, action: DialogAction) -> Command<EditorMessage> {
        if index >= self.tabs.len() {
            return Command::none();
        }
        self.start_dialog(action)
    }

    fn save_tab_existing(&mut self, index: usize) -> bool {
        if index >= self.tabs.len() || self.tabs[index].editor.path().is_none() {
            return false;
        }
        match self.tabs[index].editor.save() {
            Ok(()) => true,
            Err(error) => {
                self.tabs[index].editor.set_status(format!("{error}"));
                false
            }
        }
    }

    fn save_to_path(&mut self, index: usize, path: &Path) -> bool {
        if index >= self.tabs.len() {
            return false;
        }
        match self.tabs[index].editor.save_as(path) {
            Ok(()) => true,
            Err(error) => {
                self.tabs[index].editor.set_status(format!("{error}"));
                false
            }
        }
    }

    fn handle_dialog_result(
        &mut self,
        action: DialogAction,
        result: Result<Option<PathBuf>, String>,
    ) -> Command<EditorMessage> {
        self.pending_dialog = None;
        let path = match result {
            Ok(Some(path)) => path,
            Ok(None) => return Command::none(),
            Err(error) => {
                self.active_editor_mut()
                    .set_status(format!("file dialog: {error}"));
                return Command::none();
            }
        };

        match action {
            DialogAction::Open => self.open_file(path),
            DialogAction::Save { index, close_after } => {
                if self.save_to_path(index, &path) && close_after {
                    self.modal = None;
                    self.close_tab(index);
                }
            }
            DialogAction::SaveAs { index } => {
                self.save_to_path(index, &path);
            }
            DialogAction::SaveAll { index } => {
                if self.save_to_path(index, &path) {
                    return self.save_all();
                }
            }
        }
        Command::none()
    }

    fn request_quit(&mut self) {
        if self.dirty_count() > 0 {
            self.modal = Some(Modal::Quit);
        } else {
            self.quit = true;
        }
    }

    fn request_close_tab(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }
        if self.tabs[index].editor.dirty() {
            self.modal = Some(Modal::CloseTab(index));
        } else {
            self.close_tab(index);
        }
    }

    fn modal_save(&mut self) -> Command<EditorMessage> {
        match self.modal {
            Some(Modal::CloseTab(index)) => {
                if index >= self.tabs.len() {
                    self.modal = None;
                    return Command::none();
                }
                if self.tabs[index].editor.path().is_some() {
                    if self.save_tab_existing(index) {
                        self.modal = None;
                        self.close_tab(index);
                    }
                    return Command::none();
                }
                self.start_save_dialog(
                    index,
                    DialogAction::Save {
                        index,
                        close_after: true,
                    },
                )
            }
            Some(Modal::Quit) => self.save_all(),
            None => Command::none(),
        }
    }

    fn modal_discard(&mut self) {
        match self.modal.take() {
            Some(Modal::CloseTab(index)) => self.close_tab(index),
            Some(Modal::Quit) => self.quit = true,
            None => {}
        }
    }

    fn open_search(&mut self, replace: bool) {
        self.modal = None;
        self.search.open = true;
        self.search.replace = replace;
        self.search.current = None;
        self.search.cached = None;
        self.search.focus_replacement = replace && !self.search.query.is_empty();
        if self.search.query.is_empty()
            && let Some(selected) = self.active_editor().selected_text()
            && !selected.is_empty()
            && !selected.contains('\n')
            && selected.chars().count() <= 160
        {
            self.search.query = selected;
        }
        self.request_focus(if self.search.focus_replacement {
            FocusTarget::Replacement
        } else {
            FocusTarget::Query
        });
    }

    fn close_search(&mut self) {
        self.search.open = false;
        self.search.replace = false;
        self.search.current = None;
        self.search.focus_replacement = false;
        self.search.matches.clear();
        self.search.cached = None;
        self.focus = None;
    }

    fn sync_search(&mut self) {
        if !self.search.open || self.search.query.is_empty() {
            self.search.matches.clear();
            self.search.current = None;
            self.search.cached = None;
            return;
        }
        let editor = &self.tabs[self.active].editor;
        let revision = editor.document().revision();
        if self
            .search
            .cached
            .as_ref()
            .is_some_and(|(cached_revision, query, case)| {
                *cached_revision == revision
                    && query == &self.search.query
                    && *case == self.search.case_sensitive
            })
        {
            return;
        }
        let case_sensitive = self.search.case_sensitive;
        let matches = editor
            .document()
            .find_matches(self.search.query.as_str(), case_sensitive);
        let len = matches.len();
        self.search.matches = matches;
        self.search.cached = Some((revision, self.search.query.clone(), case_sensitive));
        if self.search.current.is_some_and(|index| index >= len) {
            self.search.current = None;
        }
    }

    fn next_match_index(&self, cursor: usize) -> Option<usize> {
        if self.search.matches.is_empty() {
            None
        } else {
            Some(
                self.search
                    .matches
                    .iter()
                    .position(|range| range.start >= cursor)
                    .unwrap_or(0),
            )
        }
    }

    fn find_next(&mut self) {
        self.sync_search();
        if self.search.matches.is_empty() {
            return;
        }
        let index = match self.search.current {
            Some(index) => (index + 1) % self.search.matches.len(),
            None => self
                .next_match_index(self.active_editor().cursor_offset())
                .unwrap_or(0),
        };
        self.select_match(index);
    }

    fn find_previous(&mut self) {
        self.sync_search();
        if self.search.matches.is_empty() {
            return;
        }
        let index = match self.search.current {
            Some(index) => {
                if index == 0 {
                    self.search.matches.len() - 1
                } else {
                    index - 1
                }
            }
            None => {
                let cursor = self.active_editor().cursor_offset();
                self.search
                    .matches
                    .iter()
                    .rposition(|range| range.start < cursor)
                    .unwrap_or(self.search.matches.len() - 1)
            }
        };
        self.select_match(index);
    }

    fn select_match(&mut self, index: usize) {
        let Some(range) = self.search.matches.get(index).cloned() else {
            return;
        };
        self.active_editor_mut().select_range(range);
        self.search.current = Some(index);
    }

    fn replace_current(&mut self) {
        if self.search.matches.is_empty() {
            self.refresh();
        }
        let cursor = self.active_editor().cursor_offset();
        let Some(index) = self
            .search
            .current
            .or_else(|| self.next_match_index(cursor))
        else {
            self.refresh();
            return;
        };
        let range = self.search.matches[index].clone();
        let replacement = self.search.replacement.clone();
        self.active_editor_mut().select_range(range);
        self.active_editor_mut().insert_text(&replacement);
        self.search.current = None;
        self.refresh();
        let cursor = self.active_editor().cursor_offset();
        self.search.current = self.next_match_index(cursor);
        self.select_match(self.search.current.unwrap_or(0));
    }

    fn replace_all(&mut self) {
        self.sync_search();
        if self.search.matches.is_empty() {
            return;
        }
        let text = self.tabs[self.active].editor.document().text();
        let matches = self.search.matches.clone();
        let replacement = self.search.replacement.clone();
        let first = matches[0].start;
        let last = matches.last().map_or(first, |range| range.end);
        let replaced = replace_matches_in(&text, &matches, &replacement);
        if replaced == text[first..last] {
            return;
        }
        let editor = self.active_editor_mut();
        editor.select_range(first..last);
        editor.insert_text(&replaced);
        self.search.current = None;
    }

    fn copy(&mut self) {
        let Some(selected) = self.active_editor().copy() else {
            self.active_editor_mut().set_status("nothing to copy");
            return;
        };
        if let Some(clipboard) = &self.clipboard {
            if let Err(error) = clipboard.set_text(&selected) {
                self.active_editor_mut()
                    .set_status(format!("clipboard: {error}"));
            } else {
                self.active_editor_mut().clear_status();
            }
        } else {
            self.active_editor_mut()
                .set_status("system clipboard unavailable");
        }
    }

    fn cut(&mut self) {
        let Some(selected) = self.active_editor_mut().cut() else {
            self.active_editor_mut().set_status("nothing to cut");
            return;
        };
        if let Some(clipboard) = &self.clipboard {
            if let Err(error) = clipboard.set_text(&selected) {
                self.active_editor_mut()
                    .set_status(format!("clipboard: {error}"));
                self.active_editor_mut().undo();
            }
        } else {
            self.active_editor_mut()
                .set_status("system clipboard unavailable");
            self.active_editor_mut().undo();
        }
    }

    fn paste(&mut self) {
        let Some(clipboard) = &self.clipboard else {
            self.active_editor_mut()
                .set_status("system clipboard unavailable");
            return;
        };
        match clipboard.get_text() {
            Ok(Some(text)) => self.active_editor_mut().paste(&text),
            Ok(None) => self.active_editor_mut().set_status("clipboard is empty"),
            Err(error) => self
                .active_editor_mut()
                .set_status(format!("clipboard: {error}")),
        }
    }
}

impl App for EditorModel {
    type Message = EditorMessage;
    type View = EditorSnapshot;

    fn update(&mut self, message: EditorMessage) -> Command<EditorMessage> {
        // A native picker is modal. Ignore concurrent edits that could
        // invalidate the tab index captured when the dialog started.
        if self.pending_dialog.is_some() && !matches!(&message, EditorMessage::DialogResult { .. })
        {
            return Command::none();
        }
        if self.search.open && matches!(message, EditorMessage::Tab) {
            self.search.focus_replacement = self.search.replace && !self.search.focus_replacement;
            self.request_focus(if self.search.focus_replacement {
                FocusTarget::Replacement
            } else {
                FocusTarget::Query
            });
            self.refresh();
            return Command::none();
        }
        if (self.search.open || self.modal.is_some()) && is_editor_input(&message) {
            return Command::none();
        }
        match message {
            EditorMessage::Quit | EditorMessage::CloseRequested => self.request_quit(),
            EditorMessage::New => self.new_tab(),
            EditorMessage::Open => {
                self.refresh();
                return self.open_dialog();
            }
            EditorMessage::Save => {
                let command = self.save_current();
                self.refresh();
                return command;
            }
            EditorMessage::SaveAs => {
                self.refresh();
                return self.save_as_dialog();
            }
            EditorMessage::SelectTab(index) => self.select_tab(index),
            EditorMessage::NextTab => self.cycle_tab(1),
            EditorMessage::PreviousTab => self.cycle_tab(-1),
            EditorMessage::CloseTab(index) => self.request_close_tab(index),
            EditorMessage::CloseActiveTab => self.request_close_tab(self.active),
            EditorMessage::Undo => {
                self.active_editor_mut().undo();
                self.search.current = None;
            }
            EditorMessage::Redo => {
                self.active_editor_mut().redo();
                self.search.current = None;
            }
            EditorMessage::Copy => self.copy(),
            EditorMessage::Cut => {
                self.cut();
                self.search.current = None;
            }
            EditorMessage::Paste => {
                self.paste();
                self.search.current = None;
            }
            EditorMessage::SelectAll => self.active_editor_mut().select_all(),
            EditorMessage::Insert(text) => {
                self.active_editor_mut().insert_text(&text);
                self.search.current = None;
            }
            EditorMessage::Backspace => {
                self.active_editor_mut().backspace();
                self.search.current = None;
            }
            EditorMessage::DeleteForward => {
                self.active_editor_mut().delete_forward();
                self.search.current = None;
            }
            EditorMessage::Enter => {
                self.active_editor_mut().enter();
                self.search.current = None;
            }
            EditorMessage::Tab => {
                self.active_editor_mut().tab();
                self.search.current = None;
            }
            EditorMessage::MoveLeft(extend) => self.active_editor_mut().move_left(extend),
            EditorMessage::MoveRight(extend) => self.active_editor_mut().move_right(extend),
            EditorMessage::MoveUp(extend) => self.active_editor_mut().move_up(extend),
            EditorMessage::MoveDown(extend) => self.active_editor_mut().move_down(extend),
            EditorMessage::MoveHome(extend) => self.active_editor_mut().move_home(extend),
            EditorMessage::MoveEnd(extend) => self.active_editor_mut().move_end(extend),
            EditorMessage::MoveDocumentStart(extend) => {
                self.active_editor_mut().move_document_start(extend)
            }
            EditorMessage::MoveDocumentEnd(extend) => {
                self.active_editor_mut().move_document_end(extend)
            }
            EditorMessage::PageUp(extend) => self.active_editor_mut().move_page_up(extend),
            EditorMessage::PageDown(extend) => self.active_editor_mut().move_page_down(extend),
            EditorMessage::Pointer {
                row,
                column,
                extend,
            } => self.active_editor_mut().click(row, column, extend),
            EditorMessage::Scroll { columns, rows } => {
                self.active_editor_mut().scroll(columns, rows)
            }
            EditorMessage::Viewport { rows, columns } => {
                self.viewport = (rows.max(1), columns.max(1));
                for tab in &mut self.tabs {
                    tab.editor.set_viewport(rows.max(1), columns.max(1));
                }
            }
            EditorMessage::PollWrap => {
                for tab in &mut self.tabs {
                    tab.editor.poll_wrap();
                }
            }
            EditorMessage::ToggleWrap => {
                let wrap = !self.active_editor().wrap();
                self.active_editor_mut().set_wrap(wrap);
                self.search.current = None;
            }
            EditorMessage::Find => self.open_search(false),
            EditorMessage::Replace => self.open_search(true),
            EditorMessage::CloseSearch => self.close_search(),
            EditorMessage::SearchQuery(query) => {
                self.search.query = query;
                self.search.current = None;
                self.search.focus_replacement = false;
                self.search.cached = None;
            }
            EditorMessage::Replacement(replacement) => {
                self.search.replacement = replacement;
                self.search.focus_replacement = true;
            }
            EditorMessage::ToggleCase => {
                self.search.case_sensitive = !self.search.case_sensitive;
                self.search.current = None;
                self.search.cached = None;
            }
            EditorMessage::FindNext => self.find_next(),
            EditorMessage::FindPrevious => self.find_previous(),
            EditorMessage::FocusReplacement => {
                if self.search.open && self.search.replace {
                    self.search.focus_replacement = true;
                    self.request_focus(FocusTarget::Replacement);
                }
            }
            EditorMessage::ReplaceCurrent => self.replace_current(),
            EditorMessage::ReplaceAll => self.replace_all(),
            EditorMessage::ModalSave => {
                let command = self.modal_save();
                self.refresh();
                return command;
            }
            EditorMessage::ModalDiscard => self.modal_discard(),
            EditorMessage::ModalCancel => self.modal = None,
            EditorMessage::Escape => {
                if self.modal.is_some() {
                    self.modal = None;
                } else if self.search.open {
                    self.close_search();
                }
            }
            EditorMessage::DialogResult { action, result } => {
                let command = self.handle_dialog_result(action, result);
                self.refresh();
                return command;
            }
        }
        self.refresh();
        Command::none()
    }

    fn view(&self) -> EditorSnapshot {
        self.snapshot.clone()
    }

    fn should_quit(&self) -> bool {
        self.quit
    }
}

fn is_editor_input(message: &EditorMessage) -> bool {
    matches!(
        message,
        EditorMessage::Undo
            | EditorMessage::Redo
            | EditorMessage::Copy
            | EditorMessage::Cut
            | EditorMessage::Paste
            | EditorMessage::SelectAll
            | EditorMessage::Insert(_)
            | EditorMessage::Backspace
            | EditorMessage::DeleteForward
            | EditorMessage::Enter
            | EditorMessage::Tab
            | EditorMessage::MoveLeft(_)
            | EditorMessage::MoveRight(_)
            | EditorMessage::MoveUp(_)
            | EditorMessage::MoveDown(_)
            | EditorMessage::MoveHome(_)
            | EditorMessage::MoveEnd(_)
            | EditorMessage::MoveDocumentStart(_)
            | EditorMessage::MoveDocumentEnd(_)
            | EditorMessage::PageUp(_)
            | EditorMessage::PageDown(_)
            | EditorMessage::Pointer { .. }
    )
}

fn tab_name(tab: &Tab) -> String {
    tab.editor
        .path()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .unwrap_or("Untitled")
        .to_string()
}

pub fn replace_matches_in(text: &str, matches: &[Range<usize>], replacement: &str) -> String {
    let first = matches.first().map_or(0, |range| range.start);
    let last = matches.last().map_or(first, |range| range.end);
    let mut replaced = String::with_capacity(last.saturating_sub(first));
    let mut cursor = first;
    for range in matches {
        replaced.push_str(&text[cursor..range.start]);
        replaced.push_str(replacement);
        cursor = range.end;
    }
    replaced.push_str(&text[cursor..last]);
    replaced
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "ferriso-editor-{tag}-{}-{}.txt",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ))
    }

    #[test]
    fn synchronous_save_refreshes_the_snapshot() {
        use ferriso_platform::MemoryFileDialog;

        let path = temp_path("sync-save");
        std::fs::write(&path, "").expect("fixture");
        let mut model =
            EditorModel::with_dialog(Some(path.clone()), Arc::new(MemoryFileDialog::new()));
        model.update(EditorMessage::Insert("hello".into()));
        model.update(EditorMessage::Save);

        assert_eq!(model.view().view.path.as_deref(), Some(path.as_path()));
        assert!(!model.view().view.dirty);
        assert_eq!(std::fs::read_to_string(&path).expect("saved"), "hello");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn synchronous_close_save_refreshes_the_snapshot() {
        use ferriso_platform::MemoryFileDialog;

        let path = temp_path("sync-close-save");
        std::fs::write(&path, "").expect("fixture");
        let mut model =
            EditorModel::with_dialog(Some(path.clone()), Arc::new(MemoryFileDialog::new()));
        model.update(EditorMessage::Insert("hello".into()));
        model.update(EditorMessage::CloseTab(0));
        model.update(EditorMessage::ModalSave);

        assert!(model.view().modal.is_none());
        assert!(model.view().view.path.is_none());
        assert!(!model.view().view.dirty);
        assert_eq!(std::fs::read_to_string(&path).expect("saved"), "hello");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn pending_dialog_freezes_tab_mutations() {
        let mut model = EditorModel::new(None);
        model.update(EditorMessage::Insert("x".into()));
        model.pending_dialog = Some(DialogAction::Save {
            index: 0,
            close_after: false,
        });

        model.update(EditorMessage::CloseTab(0));
        assert_eq!(model.view().tabs.len(), 1);

        model.update(EditorMessage::DialogResult {
            action: DialogAction::Save {
                index: 0,
                close_after: false,
            },
            result: Ok(None),
        });
        assert!(model.pending_dialog.is_none());
    }

    #[test]
    fn save_all_dialog_finishes_quit_after_the_tab_is_saved() {
        use ferriso_platform::MemoryFileDialog;

        let path = temp_path("save-all");
        let dialog: Arc<dyn FileDialog> = Arc::new(MemoryFileDialog::new().with_save(&path));
        let mut model = EditorModel::with_dialog(None, dialog);
        model.update(EditorMessage::Insert("hello".into()));
        model.update(EditorMessage::Quit);
        model.update(EditorMessage::ModalSave);
        assert_eq!(
            model.pending_dialog,
            Some(DialogAction::SaveAll { index: 0 })
        );

        model.update(EditorMessage::DialogResult {
            action: DialogAction::SaveAll { index: 0 },
            result: Ok(Some(path.clone())),
        });
        assert!(model.should_quit());
        assert!(model.pending_dialog.is_none());
        assert_eq!(std::fs::read_to_string(&path).expect("saved"), "hello");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn close_after_save_dialog_closes_the_saved_tab() {
        use ferriso_platform::MemoryFileDialog;

        let path = temp_path("close-save");
        let dialog: Arc<dyn FileDialog> = Arc::new(MemoryFileDialog::new().with_save(&path));
        let mut model = EditorModel::with_dialog(None, dialog);
        model.update(EditorMessage::Insert("hello".into()));
        model.update(EditorMessage::CloseTab(0));
        model.update(EditorMessage::ModalSave);
        assert_eq!(
            model.pending_dialog,
            Some(DialogAction::Save {
                index: 0,
                close_after: true,
            })
        );

        model.update(EditorMessage::DialogResult {
            action: DialogAction::Save {
                index: 0,
                close_after: true,
            },
            result: Ok(Some(path.clone())),
        });
        assert!(model.view().modal.is_none());
        assert!(model.view().view.path.is_none());
        assert!(!model.view().view.dirty);
        assert_eq!(std::fs::read_to_string(&path).expect("saved"), "hello");
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn open_dialog_feeds_the_actor_asynchronously() {
        use ferriso_platform::MemoryFileDialog;

        let path = temp_path("open");
        std::fs::write(&path, "hello").expect("fixture");
        let dialog: Arc<dyn FileDialog> = Arc::new(MemoryFileDialog::new().with_file(&path));
        let (handle, mut snapshots, task) =
            ferriso_core::spawn(EditorModel::with_dialog(None, dialog));

        assert!(handle.send(EditorMessage::Open));
        let view = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                let view = snapshots.changed().await.expect("actor alive");
                if view.view.path.as_deref() == Some(path.as_path()) {
                    break view;
                }
            }
        })
        .await
        .expect("open result");
        assert_eq!(view.view.line_count, 1);

        handle.shutdown();
        task.join().await;
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn save_dialog_writes_without_running_on_the_actor() {
        use ferriso_platform::MemoryFileDialog;

        let path = temp_path("save");
        let dialog: Arc<dyn FileDialog> = Arc::new(MemoryFileDialog::new().with_save(&path));
        let (handle, mut snapshots, task) =
            ferriso_core::spawn(EditorModel::with_dialog(None, dialog));

        assert!(handle.send(EditorMessage::Insert("hello".into())));
        assert!(handle.send(EditorMessage::Save));
        let view = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                let view = snapshots.changed().await.expect("actor alive");
                if view.view.path.as_deref() == Some(path.as_path()) && !view.view.dirty {
                    break view;
                }
            }
        })
        .await
        .expect("save result");
        assert_eq!(view.view.line_count, 1);
        assert_eq!(std::fs::read_to_string(&path).expect("saved"), "hello");

        handle.shutdown();
        task.join().await;
        let _ = std::fs::remove_file(path);
    }

    fn find_matches(text: &str, query: &str, case_sensitive: bool) -> Vec<Range<usize>> {
        ferriso_editor::Document::from_text(text).find_matches(query, case_sensitive)
    }

    #[test]
    fn closing_a_dirty_editor_prompts_instead_of_quitting() {
        let mut model = EditorModel::new(None);
        model.update(EditorMessage::Insert("x".into()));
        assert!(model.view().view.dirty);

        model.update(EditorMessage::Quit);
        assert!(matches!(model.view().modal, Some(Modal::Quit)));
        assert!(!model.should_quit());

        model.update(EditorMessage::ModalCancel);
        assert!(model.view().modal.is_none());

        model.update(EditorMessage::Quit);
        model.update(EditorMessage::ModalDiscard);
        assert!(model.should_quit());
    }

    #[test]
    fn search_is_case_sensitive_when_requested() {
        assert_eq!(find_matches("One one ONE", "one", true), vec![4..7]);
    }

    #[test]
    fn search_folds_case_without_losing_byte_offsets() {
        let matches = find_matches("Café café CAFÉ", "café", false);
        assert_eq!(matches.len(), 3);
        assert_eq!(&"Café café CAFÉ"[matches[2].clone()], "CAFÉ");
    }

    #[test]
    fn replace_matches_keeps_unmatched_text() {
        let text = "alpha beta alpha";
        let matches = find_matches(text, "alpha", true);
        assert_eq!(replace_matches_in(text, &matches, "X"), "X beta X");
    }

    #[test]
    fn replace_matches_can_delete() {
        let text = "alpha beta alpha";
        let matches = find_matches(text, "alpha ", true);
        assert_eq!(replace_matches_in(text, &matches, ""), "");
    }
}
