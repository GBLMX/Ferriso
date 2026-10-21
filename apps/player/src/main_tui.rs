mod app;

fn main() -> std::io::Result<()> {
    ferriso_ratatui::run(app::Player::new(), ferriso_ratatui::Options::default())
}
