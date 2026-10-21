use ferriso_player::app::Player;
use ferriso_player::config::AppConfig;
use ferriso_player::gui::PlayerGui;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = match AppConfig::load() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("{}", ferriso_diagnostics::render(&error));
            std::process::exit(2);
        }
    };
    ferriso_gui::run(
        Player::with_config(config.player),
        PlayerGui::new(),
        config.gui.settings(),
    )?;
    Ok(())
}
