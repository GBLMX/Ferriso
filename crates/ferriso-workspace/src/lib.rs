//! Frontend-neutral workspace models: tabs, a two-pane split, and a
//! virtual-table viewport.
//!
//! This crate deliberately has no toolkit dependency. TUI, iced, GPUI, and
//! Floem applications can share the same lifecycle rules without linking a
//! renderer. These are state models, not widgets. Applications keep their
//! content, layout, focus routing, and rendering in the adapter; the models
//! only protect the invariants that repeatedly cause bugs in editors,
//! terminals, and data tools:
//!
//! - closing the active tab selects a neighbor;
//! - a split ratio stays usable;
//! - a table renders only its visible rows while preserving selection.
//!
//! Full recursive pane trees and rendered table columns remain application
//! policy. Use a toolkit's pane grid when an application needs arbitrary splits.

use std::ops::Range;

/// A bounded set of tabs with exactly one active index when non-empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tabs<T> {
    tabs: Vec<T>,
    active: Option<usize>,
}

impl<T> Default for Tabs<T> {
    fn default() -> Self {
        Self {
            tabs: Vec::new(),
            active: None,
        }
    }
}

impl<T> Tabs<T> {
    /// Create an empty tab set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a tab set and activate the first tab.
    pub fn from_tabs(tabs: impl IntoIterator<Item = T>) -> Self {
        let tabs: Vec<T> = tabs.into_iter().collect();
        let active = (!tabs.is_empty()).then_some(0);
        Self { tabs, active }
    }

    /// Number of tabs.
    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    /// Whether there are no tabs.
    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    /// Active index, or `None` when empty.
    pub fn active(&self) -> Option<usize> {
        self.active
    }

    /// Active tab.
    pub fn active_tab(&self) -> Option<&T> {
        self.active.and_then(|index| self.tabs.get(index))
    }

    /// Mutable active tab.
    pub fn active_tab_mut(&mut self) -> Option<&mut T> {
        self.active.and_then(|index| self.tabs.get_mut(index))
    }

    /// All tabs in display order.
    pub fn tabs(&self) -> &[T] {
        &self.tabs
    }

    /// One tab by index.
    pub fn get(&self, index: usize) -> Option<&T> {
        self.tabs.get(index)
    }

    /// Mutable tab by index.
    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        self.tabs.get_mut(index)
    }

    /// Append a tab and activate it. Returns its index.
    pub fn push(&mut self, tab: T) -> usize {
        self.tabs.push(tab);
        let index = self.tabs.len() - 1;
        self.active = Some(index);
        index
    }

    /// Select a tab by index. Returns whether the index existed.
    pub fn select(&mut self, index: usize) -> bool {
        if index >= self.tabs.len() {
            return false;
        }
        self.active = Some(index);
        true
    }

    /// Select the next tab, wrapping at the end.
    pub fn select_next(&mut self) -> Option<usize> {
        let len = self.tabs.len();
        if len == 0 {
            return None;
        }
        let active = self.active.unwrap_or(0);
        self.active = Some((active + 1) % len);
        self.active
    }

    /// Select the previous tab, wrapping at the start.
    pub fn select_previous(&mut self) -> Option<usize> {
        let len = self.tabs.len();
        if len == 0 {
            return None;
        }
        let active = self.active.unwrap_or(0);
        self.active = Some((active + len - 1) % len);
        self.active
    }

    /// Remove a tab and repair the active index.
    ///
    /// Closing the active tab selects the tab that took its place, or the new
    /// last tab when it was at the end. Closing the last remaining tab leaves
    /// the set empty.
    pub fn close(&mut self, index: usize) -> Option<T> {
        if index >= self.tabs.len() {
            return None;
        }

        let active = self.active?;
        let closing_active = index == active;
        let closing_before_active = index < active;
        let removed = self.tabs.remove(index);

        if self.tabs.is_empty() {
            self.active = None;
        } else if closing_active {
            self.active = Some(index.min(self.tabs.len() - 1));
        } else if closing_before_active {
            self.active = Some(active - 1);
        }

        Some(removed)
    }

    /// Close the active tab, if any.
    pub fn close_active(&mut self) -> Option<T> {
        self.active.and_then(|index| self.close(index))
    }

    /// Remove all tabs.
    pub fn clear(&mut self) {
        self.tabs.clear();
        self.active = None;
    }
}

/// Direction of a two-pane split.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SplitAxis {
    /// Panes are side by side; the ratio is the first pane's width share.
    #[default]
    Columns,
    /// Panes are stacked; the ratio is the first pane's height share.
    Rows,
}

/// Which pane of a [`SplitPane`] has focus.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PaneFocus {
    #[default]
    First,
    Second,
}

impl PaneFocus {
    /// The other pane.
    pub fn other(self) -> Self {
        match self {
            Self::First => Self::Second,
            Self::Second => Self::First,
        }
    }
}

/// The smallest ratio a split may take. This keeps both panes usable.
pub const MIN_SPLIT_RATIO: f32 = 0.1;
/// The largest ratio a split may take.
pub const MAX_SPLIT_RATIO: f32 = 0.9;

/// A fixed two-pane split with a ratio and focus.
///
/// Arbitrary recursive layouts stay in the application (or Iced's
/// `pane_grid`); this model covers the common sidebar/preview or list/detail
/// split without duplicating a layout tree.
#[derive(Debug, Clone, PartialEq)]
pub struct SplitPane<K> {
    first: K,
    second: K,
    axis: SplitAxis,
    ratio: f32,
    focus: PaneFocus,
}

impl<K> SplitPane<K> {
    /// Create a split with the first pane focused.
    pub fn new(first: K, second: K, axis: SplitAxis) -> Self {
        Self {
            first,
            second,
            axis,
            ratio: 0.5,
            focus: PaneFocus::First,
        }
    }

    /// First pane content.
    pub fn first(&self) -> &K {
        &self.first
    }

    /// Second pane content.
    pub fn second(&self) -> &K {
        &self.second
    }

    /// Split direction.
    pub fn axis(&self) -> SplitAxis {
        self.axis
    }

    /// Change the split direction without changing the ratio.
    pub fn set_axis(&mut self, axis: SplitAxis) {
        self.axis = axis;
    }

    /// First pane's share, clamped to [`MIN_SPLIT_RATIO`]..=[`MAX_SPLIT_RATIO`].
    pub fn ratio(&self) -> f32 {
        self.ratio
    }

    /// Set the split ratio. Non-finite values are ignored.
    pub fn set_ratio(&mut self, ratio: f32) {
        if ratio.is_finite() {
            self.ratio = ratio.clamp(MIN_SPLIT_RATIO, MAX_SPLIT_RATIO);
        }
    }

    /// Move the split by `delta` ratio units.
    pub fn resize_by(&mut self, delta: f32) {
        if delta.is_finite() {
            self.set_ratio(self.ratio + delta);
        }
    }

    /// Focused pane.
    pub fn focus(&self) -> PaneFocus {
        self.focus
    }

    /// Focus a specific pane.
    pub fn focus_pane(&mut self, focus: PaneFocus) {
        self.focus = focus;
    }

    /// Move focus to the other pane.
    pub fn toggle_focus(&mut self) {
        self.focus = self.focus.other();
    }
}

/// Number of rows rendered beyond each edge of a [`TableState`] viewport.
pub const TABLE_OVERSCAN: usize = 4;

/// Virtual-table viewport state for a fixed row height.
///
/// The application owns the row data and rendering. The model only tracks row
/// count, scroll offset, selection, and the visible range. This keeps a
/// 100,000-row result set cheap: build widgets for [`Self::visible_range`] and
/// use spacers for the rows outside it. Feed the renderer's scroll position
/// into [`Self::set_scroll_offset`], and sync a programmatic
/// [`Self::reveal_selected`] back to the scrollable's scroll operation.
#[derive(Debug, Clone, PartialEq)]
pub struct TableState {
    row_count: usize,
    row_height: f32,
    viewport_height: f32,
    scroll_offset: f32,
    selected: Option<usize>,
    overscan: usize,
}

impl Default for TableState {
    fn default() -> Self {
        Self::new(28.0, 320.0)
    }
}

impl TableState {
    /// Create a viewport. Row height is clamped to at least one logical pixel;
    /// viewport height may be zero.
    pub fn new(row_height: f32, viewport_height: f32) -> Self {
        Self {
            row_count: 0,
            row_height: if row_height.is_finite() {
                row_height.max(1.0)
            } else {
                1.0
            },
            viewport_height: if viewport_height.is_finite() {
                viewport_height.max(0.0)
            } else {
                0.0
            },
            scroll_offset: 0.0,
            selected: None,
            overscan: TABLE_OVERSCAN,
        }
    }

    /// Number of rows in the data set.
    pub fn row_count(&self) -> usize {
        self.row_count
    }

    /// Replace the data-set length, repairing selection and scroll offset.
    pub fn set_row_count(&mut self, row_count: usize) {
        self.row_count = row_count;
        if row_count == 0 {
            self.selected = None;
            self.scroll_offset = 0.0;
            return;
        }
        if self.selected.is_some_and(|selected| selected >= row_count) {
            self.selected = Some(row_count - 1);
        }
        self.set_scroll_offset(self.scroll_offset);
    }

    /// Logical pixel height of one row.
    pub fn row_height(&self) -> f32 {
        self.row_height
    }

    /// Visible viewport height.
    pub fn viewport_height(&self) -> f32 {
        self.viewport_height
    }

    /// Resize the viewport, clamping the scroll offset to the new bounds.
    pub fn set_viewport_height(&mut self, height: f32) {
        if height.is_finite() {
            self.viewport_height = height.max(0.0);
            self.set_scroll_offset(self.scroll_offset);
        }
    }

    /// Current vertical scroll offset in logical pixels.
    pub fn scroll_offset(&self) -> f32 {
        self.scroll_offset
    }

    /// Set the scroll offset, clamped to the scrollable range.
    pub fn set_scroll_offset(&mut self, offset: f32) {
        self.scroll_offset = if offset.is_finite() {
            offset.clamp(0.0, self.max_scroll())
        } else {
            0.0
        };
    }

    /// Scroll vertically by a number of rows.
    pub fn scroll_rows(&mut self, rows: isize) {
        let offset = self.scroll_offset + rows as f32 * self.row_height;
        self.set_scroll_offset(offset);
    }

    /// Total content height.
    pub fn total_height(&self) -> f32 {
        self.row_count as f32 * self.row_height
    }

    /// Maximum valid scroll offset.
    pub fn max_scroll(&self) -> f32 {
        (self.total_height() - self.viewport_height).max(0.0)
    }

    /// Selected row, if any.
    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    /// Number of complete rows in the viewport, at least one.
    pub fn page_size(&self) -> usize {
        ((self.viewport_height / self.row_height).floor() as usize).max(1)
    }

    /// Rows the view should build, including conservative overscan.
    pub fn visible_range(&self) -> Range<usize> {
        if self.row_count == 0 {
            return 0..0;
        }
        let first = (self.scroll_offset / self.row_height).floor().max(0.0) as usize;
        let visible = (self.viewport_height / self.row_height).ceil().max(1.0) as usize;
        let start = first.saturating_sub(self.overscan).min(self.row_count);
        let end = first
            .saturating_add(visible)
            .saturating_add(self.overscan)
            .min(self.row_count);
        start..end
    }

    /// Select a row, clamping to the data set, and reveal it.
    pub fn select(&mut self, index: usize) {
        if self.row_count == 0 {
            self.selected = None;
            return;
        }
        let index = index.min(self.row_count - 1);
        self.selected = Some(index);
        self.reveal_selected();
    }

    /// Clear selection without changing scroll position.
    pub fn clear_selection(&mut self) {
        self.selected = None;
    }

    /// Move selection by rows without wrapping.
    pub fn move_selection(&mut self, delta: isize) {
        if self.row_count == 0 {
            self.selected = None;
            return;
        }
        let current = self.selected.unwrap_or(0) as isize;
        let next = (current + delta).clamp(0, self.row_count as isize - 1) as usize;
        self.selected = Some(next);
        self.reveal_selected();
    }

    /// Move selection by one page.
    pub fn move_page(&mut self, direction: isize) {
        let page = self.page_size() as isize;
        self.move_selection(page * direction.signum());
    }

    /// Select the first row.
    pub fn select_first(&mut self) {
        self.select(0);
    }

    /// Select the last row.
    pub fn select_last(&mut self) {
        if self.row_count > 0 {
            self.select(self.row_count - 1);
        }
    }

    /// Scroll only enough to make the selected row visible.
    pub fn reveal_selected(&mut self) {
        let Some(index) = self.selected else {
            return;
        };
        let top = index as f32 * self.row_height;
        let bottom = top + self.row_height;
        if top < self.scroll_offset {
            self.set_scroll_offset(top);
        } else if bottom > self.scroll_offset + self.viewport_height {
            self.set_scroll_offset(bottom - self.viewport_height);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabs_close_active_and_repair_selection() {
        let mut tabs = Tabs::from_tabs(["a", "b", "c"]);
        assert_eq!(tabs.active(), Some(0));
        assert_eq!(tabs.close_active(), Some("a"));
        assert_eq!(tabs.active(), Some(0));
        assert_eq!(tabs.active_tab(), Some(&"b"));

        assert_eq!(tabs.close(1), Some("c"));
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs.active_tab(), Some(&"b"));
        assert_eq!(tabs.close_active(), Some("b"));
        assert!(tabs.is_empty());
        assert_eq!(tabs.active(), None);
    }

    #[test]
    fn tabs_shift_selection_when_closing_before_it() {
        let mut tabs = Tabs::from_tabs(["a", "b", "c"]);
        assert!(tabs.select(2));
        assert_eq!(tabs.close(0), Some("a"));
        assert_eq!(tabs.active(), Some(1));
        assert_eq!(tabs.active_tab(), Some(&"c"));

        assert_eq!(tabs.close(0), Some("b"));
        assert_eq!(tabs.active(), Some(0));
        assert_eq!(tabs.active_tab(), Some(&"c"));
    }

    #[test]
    fn tabs_cycle_and_activate_new_tabs() {
        let mut tabs = Tabs::new();
        assert_eq!(tabs.select_next(), None);
        tabs.push("a");
        tabs.push("b");
        assert_eq!(tabs.active_tab(), Some(&"b"));
        assert_eq!(tabs.select_next(), Some(0));
        assert_eq!(tabs.select_previous(), Some(1));
        assert!(!tabs.select(9));
        assert_eq!(tabs.active(), Some(1));
    }

    #[test]
    fn split_ratio_stays_usable() {
        let mut split = SplitPane::new("files", "preview", SplitAxis::Columns);
        assert_eq!(split.ratio(), 0.5);
        split.set_ratio(0.0);
        assert_eq!(split.ratio(), MIN_SPLIT_RATIO);
        split.resize_by(10.0);
        assert_eq!(split.ratio(), MAX_SPLIT_RATIO);
        split.set_ratio(f32::NAN);
        assert_eq!(split.ratio(), MAX_SPLIT_RATIO);
        split.set_axis(SplitAxis::Rows);
        assert_eq!(split.axis(), SplitAxis::Rows);
        assert_eq!(split.ratio(), MAX_SPLIT_RATIO);
    }

    #[test]
    fn split_focus_toggles() {
        let mut split = SplitPane::new("files", "preview", SplitAxis::Columns);
        assert_eq!(split.focus(), PaneFocus::First);
        split.toggle_focus();
        assert_eq!(split.focus(), PaneFocus::Second);
        split.focus_pane(PaneFocus::First);
        assert_eq!(split.focus(), PaneFocus::First);
        assert_eq!(PaneFocus::First.other(), PaneFocus::Second);
    }

    #[test]
    fn table_visible_range_includes_overscan() {
        let mut table = TableState::new(20.0, 60.0);
        table.set_row_count(100);
        assert_eq!(table.visible_range(), 0..7);

        table.set_scroll_offset(200.0);
        assert_eq!(table.visible_range(), 6..17);
        assert_eq!(table.page_size(), 3);
    }

    #[test]
    fn table_scroll_is_bounded_and_repairs_on_shrink() {
        let mut table = TableState::new(20.0, 60.0);
        table.set_row_count(100);
        table.set_scroll_offset(10_000.0);
        assert_eq!(table.scroll_offset(), 1_940.0);

        table.select_last();
        assert_eq!(table.selected(), Some(99));
        assert_eq!(table.scroll_offset(), 1_940.0);

        table.set_row_count(2);
        assert_eq!(table.selected(), Some(1));
        assert_eq!(table.scroll_offset(), 0.0);
    }

    #[test]
    fn table_selection_clamps_and_moves_by_page() {
        let mut table = TableState::new(20.0, 60.0);
        table.set_row_count(100);
        table.select(50);
        assert_eq!(table.selected(), Some(50));
        assert_eq!(table.scroll_offset(), 960.0);

        table.move_selection(-100);
        assert_eq!(table.selected(), Some(0));
        assert_eq!(table.scroll_offset(), 0.0);

        table.move_page(1);
        assert_eq!(table.selected(), Some(3));
        table.select_last();
        assert_eq!(table.selected(), Some(99));
        table.clear_selection();
        assert_eq!(table.selected(), None);

        table.set_row_count(0);
        assert_eq!(table.selected(), None);
        assert_eq!(table.visible_range(), 0..0);
    }

    #[test]
    fn table_reveals_selection_within_viewport() {
        let mut table = TableState::new(20.0, 60.0);
        table.set_row_count(100);
        table.select(2);
        assert_eq!(table.scroll_offset(), 0.0);
        table.select(10);
        assert_eq!(table.scroll_offset(), 160.0);
        table.select(5);
        assert_eq!(table.selected(), Some(5));
        assert_eq!(table.scroll_offset(), 100.0);
    }
}
