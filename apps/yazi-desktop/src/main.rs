use std::sync::Arc;

use anyhow::Context as _;
use ferriso_diagnostics::{Diagnostics, Level, StderrLogger, TaskId};
use ferriso_yazi_desktop::app::Yazi;
use ferriso_yazi_desktop::config::Config;
use ferriso_yazi_desktop::view::YaziGui;

fn main() -> anyhow::Result<()> {
    let (config, sources) = match Config::load_detailed() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("{}", ferriso_diagnostics::render(&error));
            std::process::exit(2);
        }
    };

    let start = config.start_dir();
    let show_hidden = config.start.show_hidden;
    let settings = config.settings();
    let window_width = settings.size.0;
    let summary = config.summary(&sources);
    let debug = config.debug.enabled || config.debug.overlay;

    if debug {
        let diagnostics = Diagnostics::new().with_debug(true);
        if config.debug.stderr {
            diagnostics.set_logger(Arc::new(StderrLogger::new(Level::Debug)));
        }
        diagnostics.log(
            Level::Info,
            "yazi-desktop",
            "startup",
            TaskId::NONE,
            "configuration loaded; diagnostics enabled",
        );

        let art = config.art.clone();
        let overlay = config.debug.overlay;
        ferriso_gui::run_with_app_diagnostics(
            move || Yazi::new(start, show_hidden),
            move |diagnostics| {
                YaziGui::with_diagnostics(&art, diagnostics, overlay, summary)
                    .with_window_width(window_width)
            },
            settings,
            diagnostics,
        )
        .context("run the iced event loop (diagnostics enabled)")?;
    } else {
        ferriso_gui::run(
            Yazi::new(start, show_hidden),
            YaziGui::new(&config.art).with_window_width(window_width),
            settings,
        )
        .context("run the iced event loop")?;
    }

    Ok(())
}
