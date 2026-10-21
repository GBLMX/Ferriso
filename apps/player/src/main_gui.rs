mod app;

fn main() -> ferriso_iced::Result {
    ferriso_iced::run(app::Player::new())
}
