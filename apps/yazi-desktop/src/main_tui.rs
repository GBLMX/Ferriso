use anyhow::Context as _;
use ferriso_yazi_desktop::{app::Yazi, config::Config, tui::YaziTui};

fn main() -> anyhow::Result<()> {
    let config = match Config::load() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("{}", ferriso_diagnostics::render(&error));
            std::process::exit(2);
        }
    };
    let start = config.start_dir();
    let show_hidden = config.start.show_hidden;

    ferriso_tui::run(
        Yazi::new(start, show_hidden),
        YaziTui::new(),
        ferriso_tui::Options::default(),
    )
    .context("run the ratatui event loop")?;
    Ok(())
}
