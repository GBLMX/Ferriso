//! Terminal-local window adaptation.
//!
//! Window sizes and cell metrics are application policy; the framework only
//! supplies the resize events.

use ferriso_gui::iced::Size;

pub const CELL_WIDTH: f32 = 8.4;
pub const CELL_HEIGHT: f32 = 19.0;
pub const FONT_SIZE: f32 = 14.0;

/// Convert a logical window size into terminal columns and rows.
pub fn grid(size: Size) -> (usize, usize) {
    let columns = (size.width / CELL_WIDTH).floor().max(1.0) as usize;
    let rows = (size.height / CELL_HEIGHT).floor().max(1.0) as usize;
    (columns, rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_window_has_a_stable_grid() {
        assert_eq!(grid(Size::new(1100.0, 760.0)), (130, 40));
    }

    #[test]
    fn degenerate_sizes_keep_one_cell() {
        assert_eq!(grid(Size::new(0.0, 0.0)), (1, 1));
    }
}
