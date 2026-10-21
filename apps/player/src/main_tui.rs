use ferriso_player::app::Player;
use ferriso_player::tui::PlayerTui;
use ferriso_tui::Options;

fn main() -> std::io::Result<()> {
    ferriso_tui::run(Player::new(), PlayerTui::new(), Options::default())
}
