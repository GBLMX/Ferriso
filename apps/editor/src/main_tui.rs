// The editor model is shared with the iced binary. Compiling only the terminal
// half leaves iced-only snapshot fields and messages unused in this binary.
#![allow(dead_code)]

mod model;
mod tui;

use ferriso_core::App;

fn build() -> model::EditorModel {
    let mut model = model::EditorModel::from_args();
    if let Ok((columns, rows)) = ferriso_tui::crossterm::terminal::size() {
        model.update(model::EditorMessage::Viewport {
            rows: rows.saturating_sub(3) as usize,
            columns: columns.saturating_sub(7) as usize,
        });
    }
    model
}

fn main() -> std::io::Result<()> {
    ferriso_tui::run_with_app(
        build,
        tui::EditorTui::new(),
        ferriso_tui::Options::default(),
    )
}
