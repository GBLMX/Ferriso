//! Reusable interaction models for Ferriso GUIs.
//!
//! These are models, not a hidden router: applications keep ownership of their
//! message enum and render the returned elements with their own layout. The
//! templates cover the most common desktop patterns:
//!
//! - a fuzzy, filterable command palette with keyboard selection and paging;
//! - a searchable combobox with disabled options and keyboard selection;
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

/// One option in a [`Combobox`].
///
/// `T` is the application's value type. The model never needs to clone it; the
/// selection event reports the option's index instead.
#[derive(Debug, Clone, PartialEq)]
pub struct ComboOption<T> {
    /// Application value paired with this option. Selection events report the
    /// option index; look the value up through [`Combobox::options`].
    pub value: T,
    /// Primary label shown in the field and menu.
    pub label: String,
    /// Optional secondary line.
    pub detail: String,
    /// Disabled options remain visible but cannot be selected.
    pub disabled: bool,
}

impl<T> ComboOption<T> {
    /// Create a selectable option.
    pub fn new(value: T, label: impl Into<String>) -> Self {
        Self {
            value,
            label: label.into(),
            detail: String::new(),
            disabled: false,
        }
    }

    /// Add a secondary line.
    #[must_use]
    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = detail.into();
        self
    }

    /// Mark the option as visible but not selectable.
    #[must_use]
    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }
}

/// Keyboard intent understood by [`Combobox`].
///
/// Character input belongs to the combobox's text field, not to this enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComboKey {
    Next,
    Previous,
    First,
    Last,
    PageNext,
    PagePrevious,
    Activate,
    Dismiss,
}

/// Rows moved by Page Up / Page Down in a combobox.
pub const COMBO_PAGE: isize = 8;

impl ComboKey {
    /// Translate an iced keyboard key.
    ///
    /// Modifier chords are ignored; the application still owns global
    /// shortcuts.
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
            _ => None,
        }
    }
}

/// Result of applying a [`ComboKey`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComboEvent {
    /// The open state or highlighted row changed; redraw.
    Changed,
    /// The highlighted option was selected; its index is into the original
    /// option list.
    Selected(usize),
    /// Enter opened the closed menu.
    Opened,
    /// Escape closed the menu.
    Dismissed,
    /// Nothing responded to the key.
    Ignored,
}

/// Searchable dropdown state.
///
/// The model owns the query, open state, highlighted row, and selected index.
/// The application owns the values and decides what selecting one means. A new
/// combobox has no selection; use [`Combobox::set_selected`] or
/// [`Combobox::with_selected`] to choose a default.
#[derive(Debug, Clone)]
pub struct Combobox<T> {
    options: Vec<ComboOption<T>>,
    query: String,
    open: bool,
    highlighted: usize,
    selected: Option<usize>,
}

impl<T> Default for Combobox<T> {
    fn default() -> Self {
        Self::new([])
    }
}

impl<T> Combobox<T> {
    /// Build a closed, unselected combobox.
    pub fn new(options: impl IntoIterator<Item = ComboOption<T>>) -> Self {
        Self {
            options: options.into_iter().collect(),
            query: String::new(),
            open: false,
            highlighted: 0,
            selected: None,
        }
    }

    /// Select a default option while building the model.
    ///
    /// Invalid and disabled indices are ignored, leaving the combobox
    /// unselected.
    #[must_use]
    pub fn with_selected(mut self, index: usize) -> Self {
        let _ = self.select(index);
        self
    }

    /// All options, including disabled ones.
    pub fn options(&self) -> &[ComboOption<T>] {
        &self.options
    }

    /// Replace the option list, preserving a valid selection and highlight.
    pub fn set_options(&mut self, options: impl IntoIterator<Item = ComboOption<T>>) {
        self.options = options.into_iter().collect();
        if self
            .selected
            .is_some_and(|index| self.options.get(index).is_none_or(|option| option.disabled))
        {
            self.selected = None;
        }
        if self.options.is_empty() {
            self.close();
            return;
        }
        self.highlighted = self.first_enabled_position();
    }

    /// Whether the menu is open.
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Open the menu, clear the filter, and highlight the selected row.
    pub fn open(&mut self) {
        self.open = true;
        self.query.clear();
        let selected_position = self.selected.and_then(|selected| {
            self.filtered_indices()
                .iter()
                .position(|index| *index == selected)
        });
        self.highlighted = match selected_position {
            Some(position)
                if self
                    .selected
                    .is_some_and(|index| !self.options[index].disabled) =>
            {
                position
            }
            _ => self.first_enabled_position(),
        };
    }

    /// Close the menu and clear the filter.
    pub fn close(&mut self) {
        self.open = false;
        self.query.clear();
        self.highlighted = 0;
    }

    /// Toggle the menu.
    pub fn toggle(&mut self) {
        if self.open {
            self.close();
        } else {
            self.open();
        }
    }

    /// Current filter text.
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Update the filter text and open the menu.
    pub fn set_query(&mut self, query: impl Into<String>) {
        self.open = true;
        self.query = query.into();
        self.highlighted = self.first_enabled_position();
    }

    /// Selected option index.
    pub fn selected_index(&self) -> Option<usize> {
        self.selected
    }

    /// Selected option.
    pub fn selected(&self) -> Option<&ComboOption<T>> {
        self.selected.and_then(|index| self.options.get(index))
    }

    /// Select an option index without emitting an event.
    ///
    /// Invalid and disabled indices are ignored. A successful selection closes
    /// the menu.
    pub fn set_selected(&mut self, index: Option<usize>) {
        let Some(index) = index else {
            self.selected = None;
            self.close();
            return;
        };
        let _ = self.select(index);
    }

    /// Clear the selection and close the menu.
    pub fn clear_selection(&mut self) {
        self.selected = None;
        self.close();
    }

    /// Select a valid, enabled option and close the menu.
    ///
    /// Returns whether the option is selectable and is now selected.
    pub fn select(&mut self, index: usize) -> bool {
        if self.options.get(index).is_none_or(|option| option.disabled) {
            return false;
        }
        self.selected = Some(index);
        self.close();
        true
    }

    /// Options matching the current filter, in original order.
    pub fn filtered(&self) -> Vec<&ComboOption<T>> {
        self.filtered_indices()
            .into_iter()
            .map(|index| &self.options[index])
            .collect()
    }

    /// Highlighted option.
    pub fn highlighted(&self) -> Option<&ComboOption<T>> {
        let filtered = self.filtered_indices();
        filtered
            .get(self.highlighted)
            .map(|index| &self.options[*index])
    }

    /// Highlighted option index in the original option list.
    pub fn highlighted_index(&self) -> Option<usize> {
        self.filtered_indices().get(self.highlighted).copied()
    }

    /// Move the highlight, skipping disabled options and wrapping at the ends.
    pub fn move_highlight(&mut self, delta: isize) {
        let filtered = self.filtered_indices();
        if filtered.is_empty() {
            self.highlighted = 0;
            return;
        }

        let len = filtered.len();
        let mut position = self.highlighted.min(len - 1);
        for _ in 0..len {
            position = (position as isize + delta).rem_euclid(len as isize) as usize;
            if !self.options[filtered[position]].disabled {
                self.highlighted = position;
                return;
            }
        }
    }

    /// Apply a keyboard intent.
    pub fn handle_key(&mut self, key: ComboKey) -> ComboEvent {
        match key {
            ComboKey::Next => {
                if !self.open {
                    self.open();
                }
                self.move_highlight(1);
                ComboEvent::Changed
            }
            ComboKey::Previous => {
                if !self.open {
                    self.open();
                }
                self.move_highlight(-1);
                ComboEvent::Changed
            }
            ComboKey::First => {
                if !self.open {
                    self.open();
                }
                self.highlighted = self.first_enabled_position();
                ComboEvent::Changed
            }
            ComboKey::Last => {
                if !self.open {
                    self.open();
                }
                self.highlighted = self.last_enabled_position();
                ComboEvent::Changed
            }
            ComboKey::PageNext => {
                if !self.open {
                    self.open();
                }
                self.move_highlight(COMBO_PAGE);
                ComboEvent::Changed
            }
            ComboKey::PagePrevious => {
                if !self.open {
                    self.open();
                }
                self.move_highlight(-COMBO_PAGE);
                ComboEvent::Changed
            }
            ComboKey::Activate => {
                if !self.open {
                    self.open();
                    return ComboEvent::Opened;
                }
                let Some(index) = self.highlighted_index() else {
                    return ComboEvent::Ignored;
                };
                if self.select(index) {
                    ComboEvent::Selected(index)
                } else {
                    ComboEvent::Ignored
                }
            }
            ComboKey::Dismiss => {
                if self.open {
                    self.close();
                    ComboEvent::Dismissed
                } else {
                    ComboEvent::Ignored
                }
            }
        }
    }

    /// Render the field and, when open, an inline result menu.
    ///
    /// The returned element grows vertically while open; place it in a column
    /// with room for the menu, or compose the field and menu yourself for an
    /// overlay. Selection callbacks receive the original option index.
    ///
    /// Enter is delivered both by the field's `on_submit` callback and by
    /// [`ComboKey::Activate`]. Route those two inputs through one path in the
    /// adapter so a single keypress cannot activate twice.
    pub fn view<'a, M, F, G>(
        &'a self,
        skin: Skin,
        placeholder: &'a str,
        on_query: F,
        on_submit: M,
        on_toggle: M,
        on_select: G,
    ) -> Element<'a, M>
    where
        M: Clone + 'a,
        F: Fn(String) -> M + 'a,
        G: Fn(usize) -> M + Clone + 'a,
    {
        let tokens = skin.tokens;
        let selected_label = self.selected().map(|option| option.label.as_str());
        let placeholder = selected_label.unwrap_or(placeholder);
        let value = if self.open { self.query.as_str() } else { "" };

        let input = text_input(placeholder, value)
            .on_input(on_query)
            .on_submit(on_submit)
            .padding([8, 10])
            .width(Length::Fill)
            .style(move |_, status| design::text_input_style(skin, status));

        let toggle = button(
            text(if self.open { "▲" } else { "▼" })
                .size(10)
                .color(tokens.muted),
        )
        .padding([8, 10])
        .style(move |_, status| design::button(skin, ButtonKind::Ghost, status))
        .on_press(on_toggle);

        let mut children = column![row![input, toggle].spacing(6).align_y(Alignment::Center)]
            .spacing(6)
            .width(Length::Fill);

        if !self.open {
            return children.into();
        }

        let matches = self.filtered_indices();
        let mut menu = column![].spacing(2).width(Length::Fill);

        if matches.is_empty() {
            menu = menu.push(
                container(text("No matching options").size(11).color(tokens.muted))
                    .padding([10, 8])
                    .width(Length::Fill),
            );
        }

        for (position, index) in matches.into_iter().enumerate() {
            let option = &self.options[index];
            let highlighted = position == self.highlighted;
            let chosen = self.selected == Some(index);
            let color = if option.disabled {
                tokens.muted
            } else {
                tokens.text
            };

            let mut copy = column![text(option.label.clone()).size(12).color(color)].spacing(2);
            if !option.detail.is_empty() {
                copy = copy.push(text(option.detail.clone()).size(10).color(tokens.muted));
            }

            let mut content = row![copy.width(Length::Fill)].align_y(Alignment::Center);
            if chosen {
                content = content.push(text("✓").size(12).color(tokens.accent));
            }

            let select = on_select.clone();
            let row =
                button(content)
                    .width(Length::Fill)
                    .padding([7, 10])
                    .style(move |_, status| {
                        let mut style = design::button(
                            skin,
                            if highlighted {
                                ButtonKind::Soft
                            } else {
                                ButtonKind::Ghost
                            },
                            status,
                        );
                        if highlighted {
                            style.background =
                                Some(iced::Background::Color(design::rgba(tokens.accent, 0.18)));
                            style.border.color = design::rgba(tokens.accent, 0.34);
                        }
                        style
                    });

            let row = if option.disabled {
                row
            } else {
                row.on_press(select(index))
            };
            menu = menu.push(row);
        }

        children = children.push(
            container(scrollable(menu).height(Length::Fixed(200.0)))
                .width(Length::Fill)
                .style(move |_| design::surface(skin, Surface::Modal)),
        );
        children.into()
    }

    fn filtered_indices(&self) -> Vec<usize> {
        let query = self.query.trim().to_lowercase();
        if query.is_empty() {
            return (0..self.options.len()).collect();
        }
        self.options
            .iter()
            .enumerate()
            .filter(|(_, option)| {
                fuzzy_score(&option.label, &query).is_some()
                    || fuzzy_score(&option.detail, &query).is_some()
            })
            .map(|(index, _)| index)
            .collect()
    }

    fn first_enabled_position(&self) -> usize {
        self.filtered_indices()
            .iter()
            .position(|index| !self.options[*index].disabled)
            .unwrap_or(0)
    }

    fn last_enabled_position(&self) -> usize {
        let filtered = self.filtered_indices();
        filtered
            .iter()
            .rposition(|index| !self.options[*index].disabled)
            .unwrap_or(0)
    }
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

    fn combobox() -> Combobox<&'static str> {
        Combobox::new([
            ComboOption::new("vanilla", "Vanilla 1.21.10").detail("no mods"),
            ComboOption::new("fabric", "Fabric 1.21.10").detail("mod loader"),
            ComboOption::new("forge", "Forge 1.20.1")
                .detail("legacy")
                .disabled(),
            ComboOption::new("neoforge", "NeoForge 1.21.1").detail("modern"),
        ])
    }

    #[test]
    fn combobox_starts_closed_and_unselected() {
        let combo = combobox();
        assert!(!combo.is_open());
        assert_eq!(combo.selected_index(), None);
        assert_eq!(combo.filtered().len(), 4);
        assert_eq!(combo.highlighted_index(), Some(0));

        let disabled_first = Combobox::new([
            ComboOption::new("disabled", "Disabled").disabled(),
            ComboOption::new("enabled", "Enabled"),
        ])
        .with_selected(0);
        assert_eq!(
            disabled_first.selected_index(),
            None,
            "with_selected must reject disabled options"
        );
        let enabled = disabled_first.with_selected(1);
        assert_eq!(enabled.selected_index(), Some(1));

        let mut replaced = combobox().with_selected(0);
        replaced.set_options([ComboOption::new("blocked", "Blocked").disabled()]);
        assert_eq!(
            replaced.selected_index(),
            None,
            "a now-disabled selection is cleared"
        );
    }

    #[test]
    fn combobox_keys_map_to_navigation_intents() {
        let plain = keyboard::Modifiers::default();
        assert_eq!(
            ComboKey::from_key(&Key::Named(Named::ArrowDown), plain),
            Some(ComboKey::Next)
        );
        assert_eq!(
            ComboKey::from_key(&Key::Named(Named::ArrowUp), plain),
            Some(ComboKey::Previous)
        );
        assert_eq!(
            ComboKey::from_key(&Key::Named(Named::Home), plain),
            Some(ComboKey::First)
        );
        assert_eq!(
            ComboKey::from_key(&Key::Named(Named::End), plain),
            Some(ComboKey::Last)
        );
        assert_eq!(
            ComboKey::from_key(&Key::Named(Named::PageDown), plain),
            Some(ComboKey::PageNext)
        );
        assert_eq!(
            ComboKey::from_key(&Key::Named(Named::PageUp), plain),
            Some(ComboKey::PagePrevious)
        );
        assert_eq!(
            ComboKey::from_key(&Key::Named(Named::Enter), plain),
            Some(ComboKey::Activate)
        );
        assert_eq!(
            ComboKey::from_key(&Key::Named(Named::Escape), plain),
            Some(ComboKey::Dismiss)
        );
        assert_eq!(
            ComboKey::from_key(&Key::Named(Named::ArrowDown), keyboard::Modifiers::CTRL),
            None
        );
    }

    #[test]
    fn combobox_navigation_skips_disabled_rows_and_wraps() {
        let mut combo = combobox();
        combo.open();
        assert_eq!(combo.handle_key(ComboKey::Next), ComboEvent::Changed);
        assert_eq!(combo.highlighted_index(), Some(1));
        assert_eq!(combo.handle_key(ComboKey::Next), ComboEvent::Changed);
        assert_eq!(
            combo.highlighted_index(),
            Some(3),
            "the disabled Forge row is skipped"
        );
        assert_eq!(combo.handle_key(ComboKey::Next), ComboEvent::Changed);
        assert_eq!(combo.highlighted_index(), Some(0));
        assert_eq!(combo.handle_key(ComboKey::Previous), ComboEvent::Changed);
        assert_eq!(combo.highlighted_index(), Some(3));
        assert_eq!(combo.handle_key(ComboKey::Last), ComboEvent::Changed);
        assert_eq!(combo.highlighted_index(), Some(3));
        assert_eq!(combo.handle_key(ComboKey::First), ComboEvent::Changed);
        assert_eq!(combo.highlighted_index(), Some(0));
    }

    #[test]
    fn combobox_activate_selects_and_closes() {
        let mut combo = combobox();
        assert_eq!(combo.handle_key(ComboKey::Activate), ComboEvent::Opened);
        assert!(combo.is_open());

        combo.handle_key(ComboKey::Next);
        assert_eq!(
            combo.handle_key(ComboKey::Activate),
            ComboEvent::Selected(1)
        );
        assert_eq!(combo.selected_index(), Some(1));
        assert!(!combo.is_open());
        assert_eq!(combo.query(), "");
    }

    #[test]
    fn combobox_filter_matches_labels_and_details() {
        let mut combo = combobox();
        combo.set_query("fabric");
        assert_eq!(
            combo
                .filtered()
                .iter()
                .map(|option| option.value)
                .collect::<Vec<_>>(),
            vec!["fabric"]
        );

        combo.set_query("modern");
        assert_eq!(
            combo
                .filtered()
                .iter()
                .map(|option| option.value)
                .collect::<Vec<_>>(),
            vec!["neoforge"]
        );

        combo.set_query("zzz");
        assert!(combo.filtered().is_empty());
        assert_eq!(combo.handle_key(ComboKey::Activate), ComboEvent::Ignored);
    }

    #[test]
    fn combobox_view_monomorphizes() {
        let mut combo = combobox();
        combo.open();
        let _: Element<'_, u8> = combo.view(Skin::FERRISO, "Select a version", |_| 1, 2, 3, |_| 4);
    }

    #[test]
    fn combobox_dismiss_and_disabled_selection() {
        let mut combo = combobox();
        assert_eq!(combo.handle_key(ComboKey::Dismiss), ComboEvent::Ignored);

        combo.open();
        assert_eq!(combo.handle_key(ComboKey::Dismiss), ComboEvent::Dismissed);
        assert!(!combo.is_open());

        assert!(!combo.select(2), "disabled options cannot be selected");
        assert_eq!(combo.selected_index(), None);
        assert!(combo.select(3));
        assert_eq!(combo.selected_index(), Some(3));
        assert!(!combo.is_open());
    }

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
