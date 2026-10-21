use ferriso_gui::Settings;
use ferriso_player::app::Player;
use ferriso_player::gui::PlayerGui;

fn main() -> ferriso_gui::iced::Result {
    ferriso_gui::run(
        Player::new(),
        PlayerGui::new(),
        Settings {
            title: "Ferriso · music".into(),
            size: (1100.0, 760.0),
        },
    )
}
