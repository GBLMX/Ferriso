use ferriso_data_workbench::app::{DEFAULT_VISIBLE_ROWS, DataWorkbench};
use ferriso_data_workbench::tui::DataWorkbenchTui;

fn main() -> std::io::Result<()> {
    let rows = ferriso_tui::crossterm::terminal::size()
        .ok()
        .map(|(_, height)| height.saturating_sub(6) as usize)
        .filter(|rows| *rows > 0)
        .unwrap_or(DEFAULT_VISIBLE_ROWS);
    ferriso_tui::run(
        DataWorkbench::new().with_visible_rows(rows),
        DataWorkbenchTui::new(),
        ferriso_tui::Options::default(),
    )
}
