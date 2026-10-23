//! Reusable interaction models for Ferriso GUIs.
//!
//! These are models, not a hidden router: applications keep ownership of their
//! message enum and render the returned elements with their own layout. The
//! two templates cover the most common desktop patterns:
//!
//! - a fuzzy, filterable command palette with keyboard selection and paging;
//! - a bounded toast stack with explicit dismissal.

use std::time::Duration;

use iced::{
    Alignment, Element, Length,
    keyboard::{self, Key, key::Named},
    widget::{Id, Space, button, column, container, row, scrollable, text, text_input},
};

use crate::design::{self, ButtonKind, Skin, Surface, Tokens};

/// One command in a [`CommandPalette`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command<K> {
    /// Stable identity returned by activation.
    pub id: K,
    /// Primary label.
    pub title: String,
    /// Secondary explanation.
    pub detail: String,
    /// Optional grouping label.
    pub group: String,
    /// Optional shortcut hint.
    pub shortcut: Option<String>,
}

impl<K> Command<K> {
    /// Create a command with a title.
    pub fn new(id: K, title: impl Into<String>) -> Self {
        Self {
            id,
            title: title.into(),
            detail: String::new(),
            group: String::new(),
            shortcut: None,
        }
    }

    /// Add a detail line.
    #[must_use]
    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = detail.into();
        self
    }

    /// Add a group label.
    #[must_use]
    pub fn group(mut self, group: impl Into<String>) -> Self {
        self.group = group.into();
        self
    }

    /// Add a shortcut hint.
    #[must_use]
    pub fn shortcut(mut self, shortcut: impl Into<String>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }
}

/// A keyboard intent understood by [`CommandPalette`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteKey {
    Next,
    Previous,
    First,
    Last,
    PageNext,
    PagePrevious,
    Activate,
    Dismiss,
    Backspace,
    Character(char),
}

/// Rows moved by Page Up / Page Down in a palette.
pub const PALETTE_PAGE: isize = 8;

impl PaletteKey {
    /// Translate an iced keyboard key.
    ///
    /// Modifier chords are ignored; the application still owns global
    /// shortcuts such as `Ctrl+P`.
    pub fn from_key(key: &Key, modifiers: keyboard::Modifiers) -> Option<Self> {
        if modifiers.control() || modifiers.alt() || modifiers.logo() {
            return None;
        }

        match key {
            Key::Named(Named::ArrowDown) => Some(Self::Next),
            Key::Named(Named::ArrowUp) => Some(Self::Previous),
            Key::Named(Named::Home) => Some(Self::First),
            Key::Named(Named::End) => Some(Self::Last),
            Key::Named(Named::PageDown) => Some(Self::PageNext),
            Key::Named(Named::PageUp) => Some(Self::PagePrevious),
            Key::Named(Named::Enter) => Some(Self::Activate),
            Key::Named(Named::Escape) => Some(Self::Dismiss),
            Key::Named(Named::Backspace) => Some(Self::Backspace),
            Key::Character(character) => character.chars().next().map(Self::Character),
            _ => None,
        }
    }
}

/// Result of applying a [`PaletteKey`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaletteEvent<K> {
    /// Query or selection changed; redraw the palette.
    Changed,
    /// The selected command was activated.
    Activate(K),
    /// Escape was pressed; close the palette.
    Dismiss,
    /// No command responded to activation.
    Ignored,
}

/// Filterable command-palette state.
#[derive(Debug, Clone)]
pub struct CommandPalette<K> {
    commands: Vec<Command<K>>,
    query: String,
    selected: usize,
    input_id: Id,
}

impl<K> CommandPalette<K> {
    /// Build a palette from commands.
    pub fn new(commands: impl IntoIterator<Item = Command<K>>) -> Self {
        Self {
            commands: commands.into_iter().collect(),
            query: String::new(),
            selected: 0,
            input_id: Id::unique(),
        }
    }

    /// The text-input id, useful for focusing the query field.
    pub fn input_id(&self) -> Id {
        self.input_id.clone()
    }

    /// Current query.
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Replace the query and reset the selection.
    pub fn set_query(&mut self, query: impl Into<String>) {
        self.query = query.into();
        self.selected = 0;
    }

    /// Whether no commands exist at all.
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Commands currently matching the query.
    pub fn filtered(&self) -> Vec<&Command<K>> {
        self.filtered_indices()
            .into_iter()
            .map(|index| &self.commands[index])
            .collect()
    }

    /// The selected command, if any.
    pub fn selected(&self) -> Option<&Command<K>> {
        let indices = self.filtered_indices();
        indices
            .get(self.selected)
            .map(|index| &self.commands[*index])
    }

    /// Move the selection by one or more rows, wrapping at the ends.
    pub fn move_selection(&mut self, delta: isize) {
        let len = self.filtered_indices().len();
        if len == 0 {
            self.selected = 0;
            return;
        }
        let current = self.selected.min(len - 1) as isize;
        self.selected = (current + delta).rem_euclid(len as isize) as usize;
    }

    /// Apply a keyboard intent.
    pub fn handle_key(&mut self, key: PaletteKey) -> PaletteEvent<K>
    where
        K: Clone,
    {
        match key {
            PaletteKey::Next => {
                self.move_selection(1);
                PaletteEvent::Changed
            }
            PaletteKey::Previous => {
                self.move_selection(-1);
                PaletteEvent::Changed
            }
            PaletteKey::First => {
                self.selected = 0;
                PaletteEvent::Changed
            }
            PaletteKey::Last => {
                self.selected = self.filtered_indices().len().saturating_sub(1);
                PaletteEvent::Changed
            }
            PaletteKey::PageNext => {
                self.move_selection(PALETTE_PAGE);
                PaletteEvent::Changed
            }
            PaletteKey::PagePrevious => {
                self.move_selection(-PALETTE_PAGE);
                PaletteEvent::Changed
            }
            PaletteKey::Character(character) => {
                self.query.push(character);
                self.selected = 0;
                PaletteEvent::Changed
            }
            PaletteKey::Backspace => {
                self.query.pop();
                self.selected = 0;
                PaletteEvent::Changed
            }
            PaletteKey::Activate => self
                .selected()
                .map(|command| PaletteEvent::Activate(command.id.clone()))
                .unwrap_or(PaletteEvent::Ignored),
            PaletteKey::Dismiss => PaletteEvent::Dismiss,
        }
    }

    /// Render the palette.
    ///
    /// The returned element is a modal card. Wrap it in a [`iced::widget::Stack`]
    /// or `container` for screen placement. `on_activate` must be cloneable
    /// because each row owns a copy of the callback.
    pub fn view<'a, M, F, G>(
        &'a self,
        skin: Skin,
        placeholder: &'a str,
        on_query: F,
        on_activate: G,
    ) -> Element<'a, M>
    where
        K: Clone + 'a,
        M: Clone + 'a,
        F: Fn(String) -> M + 'a,
        G: Fn(K) -> M + Clone + 'a,
    {
        let tokens = skin.tokens;
        let mut list = column![].spacing(4);
        let matches = self.filtered_indices();

        if matches.is_empty() {
            list = list.push(
                container(text("No matching commands").size(12).color(tokens.muted))
                    .padding([14, 10])
                    .width(Length::Fill),
            );
        }

        for (position, index) in matches.into_iter().enumerate() {
            let command = &self.commands[index];
            let selected = position == self.selected;
            let activate = on_activate.clone();
            let id = command.id.clone();

            let mut copy =
                column![text(command.title.clone()).size(13).color(tokens.text)].spacing(2);
            if !command.group.is_empty() || !command.detail.is_empty() {
                let detail = if command.group.is_empty() {
                    command.detail.clone()
                } else if command.detail.is_empty() {
                    command.group.clone()
                } else {
                    format!("{} · {}", command.group, command.detail)
                };
                copy = copy.push(text(detail).size(10).color(tokens.muted));
            }

            let mut content = row![
                container(copy).width(Length::Fill),
                Space::new().width(Length::Fixed(8.0)),
            ]
            .align_y(Alignment::Center);
            if let Some(shortcut) = &command.shortcut {
                content = content.push(
                    text(shortcut.clone())
                        .size(10)
                        .font(iced::Font::MONOSPACE)
                        .color(tokens.muted),
                );
            }

            list = list.push(
                button(content)
                    .width(Length::Fill)
                    .padding([7, 10])
                    .style(move |_, status| {
                        let mut style = design::button(
                            skin,
                            if selected {
                                ButtonKind::Soft
                            } else {
                                ButtonKind::Ghost
                            },
                            status,
                        );
                        if selected {
                            style.background =
                                Some(iced::Background::Color(design::rgba(tokens.accent, 0.18)));
                            style.border.color = design::rgba(tokens.accent, 0.34);
                        }
                        style
                    })
                    .on_press(activate(id)),
            );
        }

        container(
            column![
                row![
                    design::section_label("COMMANDS", tokens),
                    Space::new().width(Length::Fill),
                    design::key_hint("esc", "close", skin),
                ]
                .align_y(Alignment::Center),
                text_input(placeholder, &self.query)
                    .id(self.input_id())
                    .on_input(on_query)
                    .padding([8, 10])
                    .size(14)
                    .style(move |_, status| design::text_input_style(skin, status)),
                scrollable(list).height(Length::Fixed(220.0)),
            ]
            .spacing(10)
            .width(Length::Fill),
        )
        .width(Length::Fixed(560.0))
        .padding(14)
        .style(move |_| design::surface(skin, Surface::Modal))
        .into()
    }

    fn filtered_indices(&self) -> Vec<usize> {
        let query = self.query.trim().to_lowercase();
        if query.is_empty() {
            return (0..self.commands.len()).collect();
        }

        let mut matches: Vec<(usize, i32)> = self
            .commands
            .iter()
            .enumerate()
            .filter_map(|(index, command)| {
                command_score(command, &query).map(|score| (index, score))
            })
            .collect();
        matches.sort_by(|(left_index, left_score), (right_index, right_score)| {
            right_score
                .cmp(left_score)
                .then_with(|| left_index.cmp(right_index))
        });
        matches.into_iter().map(|(index, _)| index).collect()
    }
}

fn command_score<K>(command: &Command<K>, query: &str) -> Option<i32> {
    let title = fuzzy_score(&command.title, query).map(|score| score + 2_000);
    let group = fuzzy_score(&command.group, query).map(|score| score + 1_000);
    let detail = fuzzy_score(&command.detail, query);
    [title, group, detail].into_iter().flatten().max()
}

/// Score an exact substring above a scattered subsequence, rewarding word
/// boundaries. Command lists are small, so rescoring on each keystroke is
/// cheaper than maintaining another index.
fn fuzzy_score(text: &str, query: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }

    let text = text.to_lowercase();
    let chars: Vec<char> = text.chars().collect();

    if let Some(byte_index) = text.find(query) {
        let index = text[..byte_index].chars().count();
        let boundary = usize::from(
            index == 0
                || chars
                    .get(index.saturating_sub(1))
                    .is_none_or(|character| !character.is_alphanumeric()),
        ) as i32
            * 40;
        return Some(1_000 - index as i32 * 3 + boundary);
    }

    let mut cursor = 0;
    let mut previous = None;
    let mut score = 0;
    for needle in query.chars() {
        let position = chars
            .get(cursor..)?
            .iter()
            .position(|value| *value == needle)?
            + cursor;
        score += 10;
        if previous.is_some_and(|previous| previous + 1 == position) {
            score += 15;
        }
        if position == 0
            || chars
                .get(position.saturating_sub(1))
                .is_none_or(|character| !character.is_alphanumeric())
        {
            score += 20;
        }
        previous = Some(position);
        cursor = position + 1;
    }

    Some(score)
}

/// Severity of a toast.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastTone {
    Info,
    Success,
    Warning,
    Danger,
}

/// One transient notification.
#[derive(Debug, Clone)]
pub struct Toast {
    /// Stable dismissal identity.
    pub id: u64,
    /// Visual severity.
    pub tone: ToastTone,
    /// Primary label.
    pub title: String,
    /// Optional detail.
    pub body: String,
    /// Time left before automatic dismissal.
    pub remaining: Duration,
}

/// Bounded toast queue with expiry and explicit dismissal.
#[derive(Debug, Default)]
pub struct Toasts {
    entries: Vec<Toast>,
    next_id: u64,
}

/// Oldest toasts fall off once this many are visible.
pub const MAX_TOASTS: usize = 5;

impl Toasts {
    /// Empty queue.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a toast and return its dismissal id.
    pub fn push(
        &mut self,
        tone: ToastTone,
        title: impl Into<String>,
        body: impl Into<String>,
        ttl: Duration,
    ) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        if self.entries.len() >= MAX_TOASTS {
            self.entries.remove(0);
        }
        self.entries.push(Toast {
            id,
            tone,
            title: title.into(),
            body: body.into(),
            remaining: ttl,
        });
        id
    }

    /// Remove one toast.
    pub fn dismiss(&mut self, id: u64) -> bool {
        let before = self.entries.len();
        self.entries.retain(|toast| toast.id != id);
        before != self.entries.len()
    }

    /// Advance timers and remove expired toasts. Returns whether the queue
    /// changed.
    pub fn tick(&mut self, elapsed: Duration) -> bool {
        let before = self.entries.len();
        self.entries.retain_mut(|toast| {
            toast.remaining = toast.remaining.saturating_sub(elapsed);
            toast.remaining > Duration::ZERO
        });
        before != self.entries.len()
    }

    /// Iterate current toasts.
    pub fn iter(&self) -> impl Iterator<Item = &Toast> {
        self.entries.iter()
    }

    /// Number of visible toasts.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no toast is visible.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Render the stack as a vertical column.
    pub fn view<'a, M>(
        &'a self,
        skin: Skin,
        on_dismiss: impl Fn(u64) -> M + Clone + 'a,
    ) -> Element<'a, M>
    where
        M: Clone + 'a,
    {
        let tokens = skin.tokens;
        let mut stack = column![].spacing(8).width(Length::Fill);
        for toast in &self.entries {
            let dismiss = on_dismiss.clone();
            let accent = tone_color(tokens, toast.tone);
            let id = toast.id;

            stack = stack.push(
                container(
                    row![
                        container(Space::new())
                            .width(Length::Fixed(3.0))
                            .height(Length::Fixed(44.0))
                            .style(move |_| container::Style {
                                background: Some(iced::Background::Color(accent)),
                                ..Default::default()
                            }),
                        column![
                            text(toast.title.clone()).size(13).color(tokens.text),
                            text(toast.body.clone()).size(11).color(tokens.muted),
                        ]
                        .spacing(2)
                        .width(Length::Fill),
                        button(text("×").size(16).color(tokens.muted))
                            .padding([0, 6])
                            .style(move |_, status| design::button(skin, ButtonKind::Ghost, status))
                            .on_press(dismiss(id)),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center),
                )
                .padding(12)
                .style(move |_| {
                    let mut style = design::surface(skin, Surface::Glass);
                    style.border.color = design::rgba(accent, 0.48);
                    style
                }),
            );
        }
        stack.into()
    }
}

fn tone_color(tokens: Tokens, tone: ToastTone) -> iced::Color {
    match tone {
        ToastTone::Info => tokens.accent,
        ToastTone::Success => tokens.success,
        ToastTone::Warning => tokens.warning,
        ToastTone::Danger => tokens.danger,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn palette() -> CommandPalette<&'static str> {
        CommandPalette::new([
            Command::new("build", "Build workspace")
                .detail("cargo build")
                .group("dev"),
            Command::new("test", "Run tests").group("dev"),
            Command::new("export", "Export report").group("docs"),
        ])
    }

    #[test]
    fn query_filters_and_selection_clamps() {
        let mut palette = palette();
        assert_eq!(palette.filtered().len(), 3);
        palette.set_query("dev");
        assert_eq!(palette.filtered().len(), 2);
        assert_eq!(palette.selected().map(|command| command.id), Some("build"));
        palette.set_query("report");
        assert_eq!(palette.selected().map(|command| command.id), Some("export"));
    }

    #[test]
    fn keyboard_navigation_wraps_and_activates() {
        let mut palette = palette();
        assert_eq!(
            palette.handle_key(PaletteKey::Previous),
            PaletteEvent::Changed
        );
        assert_eq!(
            palette.handle_key(PaletteKey::Activate),
            PaletteEvent::Activate("export")
        );
        palette.set_query("bu");
        assert_eq!(
            palette.handle_key(PaletteKey::Activate),
            PaletteEvent::Activate("build")
        );
        assert_eq!(
            palette.handle_key(PaletteKey::Backspace),
            PaletteEvent::Changed
        );
        assert_eq!(palette.query(), "b");
    }

    #[test]
    fn title_matches_outrank_group_matches() {
        let mut palette = CommandPalette::new([
            Command::new("group", "Beta").group("alpha"),
            Command::new("title", "Alpha").group("later"),
        ]);
        palette.set_query("alpha");
        assert_eq!(palette.selected().map(|command| command.id), Some("title"));
    }

    #[test]
    fn palette_keys_include_first_last_and_pages() {
        let modifiers = keyboard::Modifiers::default();
        assert_eq!(
            PaletteKey::from_key(&Key::Named(Named::Home), modifiers),
            Some(PaletteKey::First)
        );
        assert_eq!(
            PaletteKey::from_key(&Key::Named(Named::End), modifiers),
            Some(PaletteKey::Last)
        );
        assert_eq!(
            PaletteKey::from_key(&Key::Named(Named::PageDown), modifiers),
            Some(PaletteKey::PageNext)
        );
        assert_eq!(
            PaletteKey::from_key(&Key::Named(Named::PageUp), modifiers),
            Some(PaletteKey::PagePrevious)
        );
    }

    #[test]
    fn command_search_matches_scattered_characters() {
        let mut palette = palette();
        palette.set_query("bld");
        assert_eq!(
            palette.selected().map(|command| command.title.as_str()),
            Some("Build workspace")
        );
        palette.set_query("rts");
        assert_eq!(
            palette.selected().map(|command| command.title.as_str()),
            Some("Run tests")
        );
    }

    #[test]
    fn home_end_and_page_keys_move_the_selection() {
        let mut palette = palette();
        assert_eq!(palette.handle_key(PaletteKey::Last), PaletteEvent::Changed);
        assert_eq!(palette.selected().map(|command| command.id), Some("export"));
        assert_eq!(palette.handle_key(PaletteKey::First), PaletteEvent::Changed);
        assert_eq!(palette.selected().map(|command| command.id), Some("build"));
        assert_eq!(
            palette.handle_key(PaletteKey::PageNext),
            PaletteEvent::Changed
        );
        assert_eq!(palette.selected().map(|command| command.id), Some("export"));
        assert_eq!(
            palette.handle_key(PaletteKey::PagePrevious),
            PaletteEvent::Changed
        );
        assert_eq!(palette.selected().map(|command| command.id), Some("build"));
    }

    #[test]
    fn empty_palette_ignores_activation() {
        let mut palette = CommandPalette::<&str>::new([]);
        assert!(palette.is_empty());
        assert_eq!(
            palette.handle_key(PaletteKey::Activate),
            PaletteEvent::Ignored
        );
    }

    #[test]
    fn toast_stack_keeps_the_newest_entries() {
        let mut toasts = Toasts::new();
        for index in 0..(MAX_TOASTS + 2) {
            toasts.push(
                ToastTone::Info,
                format!("toast {index}"),
                "",
                Duration::from_secs(1),
            );
        }
        assert_eq!(toasts.len(), MAX_TOASTS);
        assert_eq!(
            toasts.iter().next().map(|toast| toast.title.as_str()),
            Some("toast 2")
        );
    }

    #[test]
    fn toast_timers_expire_and_dismissal_is_explicit() {
        let mut toasts = Toasts::new();
        let first = toasts.push(
            ToastTone::Info,
            "Ready",
            "loaded",
            Duration::from_millis(100),
        );
        let second = toasts.push(ToastTone::Success, "Saved", "", Duration::from_secs(10));
        assert_eq!(toasts.len(), 2);
        assert!(!toasts.tick(Duration::from_millis(50)));
        assert_eq!(toasts.len(), 2);
        assert!(toasts.tick(Duration::from_millis(60)));
        assert_eq!(toasts.len(), 1);
        assert!(toasts.dismiss(second));
        assert_eq!(toasts.len(), 0);
        assert!(!toasts.dismiss(first));
        assert!(!toasts.dismiss(999));
    }
}
