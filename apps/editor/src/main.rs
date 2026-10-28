mod app;
mod canvas;
mod model;
mod theme;

use ferriso_gui::Settings;

fn main() -> ferriso_gui::iced::Result {
    let size = (1200.0, 800.0);
    ferriso_gui::run_with_app(
        model::EditorModel::from_args,
        app::EditorRenderer::new(size),
        Settings {
            title: "Ferriso Editor".into(),
            size,
            exit_on_close_request: false,
            application_id: "io.github.gblmx.ferriso.editor".into(),
            ..Settings::default()
        },
    )
}
