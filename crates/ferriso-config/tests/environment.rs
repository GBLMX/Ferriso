//! Environment variables are the highest-precedence layer.

use std::{fs, path::PathBuf};

use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Fixture {
    gui: Gui,
    player: Player,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
struct Gui {
    title: String,
    width: f64,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
struct Player {
    volume: f64,
    tick_ms: u64,
}

impl Default for Gui {
    fn default() -> Self {
        Self {
            title: "default".into(),
            width: 100.0,
        }
    }
}

impl Default for Player {
    fn default() -> Self {
        Self {
            volume: 0.7,
            tick_ms: 100,
        }
    }
}

// One test in its own process: environment mutation is process-global and is
// deliberately kept away from the unit tests.
#[test]
fn environment_overrides_files_and_parses_scalars() {
    let dir = std::env::temp_dir().join(format!("ferriso-config-env-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create temp dir");
    let file = dir.join("explicit.toml");
    fs::write(
        &file,
        "[gui]\ntitle = \"from-file\"\nwidth = 100.0\n[player]\nvolume = 0.1\n",
    )
    .expect("write config");

    let prefix = "FERRISO_CONFIG_ENV_TEST";
    unsafe {
        std::env::set_var(format!("{prefix}__GUI__WIDTH"), "1400");
        std::env::set_var(format!("{prefix}__PLAYER__VOLUME"), "0.25");
        std::env::set_var(format!("{prefix}__PLAYER__TICK_MS"), "250");
    }

    let config: Fixture = ferriso_config::Loader::new("ferriso-config-env-test")
        .local(None::<PathBuf>)
        .explicit(&file)
        .load()
        .expect("load config");

    assert_eq!(config.gui.title, "from-file");
    assert_eq!(config.gui.width, 1400.0);
    assert_eq!(config.player.volume, 0.25);
    assert_eq!(config.player.tick_ms, 250);

    let _ = fs::remove_dir_all(&dir);
}
