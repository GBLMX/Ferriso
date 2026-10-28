//! Typed Ferriso configuration and its mapping onto the adapters.
//!
//! [`AppConfig::load`] merges the layers documented by `ferriso-config` and
//! normalizes values that come from files or the environment. Both binaries
//! read the same snapshot, so a single `ferriso.toml` (or `FERRISO__*`
//! variable) drives the terminal and the window.

use std::time::Duration;

use serde::Deserialize;

/// Default window title.
pub const DEFAULT_TITLE: &str = "Ferriso · music";

/// Entire layered Ferriso configuration.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AppConfig {
    /// Playback model: volume and tick duration.
    pub player: PlayerConfig,
    /// Terminal adapter options.
    pub tui: TuiConfig,
    /// Iced window options.
    pub gui: GuiConfig,
}

impl AppConfig {
    /// Load every configuration layer and normalize the result.
    ///
    /// Invalid numeric values are clamped instead of silently violating an
    /// adapter's assumptions (a zero poll interval would busy-loop the terminal
    /// event loop, a NaN volume would break the slider).
    pub fn load() -> Result<Self, ferriso_config::ConfigError> {
        Ok(ferriso_config::load::<Self>()?.sanitized())
    }

    /// Clamp and repair values that came from untrusted input.
    #[must_use]
    pub fn sanitized(mut self) -> Self {
        self.player.sanitize();
        self.tui.sanitize();
        self.gui.sanitize();
        self
    }
}

/// Playback model configuration.
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PlayerConfig {
    /// Initial volume, `0.0..=1.0`.
    pub volume: f64,
    /// Duration of one playback tick in milliseconds.
    pub tick_ms: u64,
}

impl Default for PlayerConfig {
    fn default() -> Self {
        Self {
            volume: 0.7,
            tick_ms: 100,
        }
    }
}

impl PlayerConfig {
    /// Tick duration used by the actor and by playback advancement.
    pub fn tick(&self) -> Duration {
        Duration::from_millis(self.tick_ms.max(1))
    }

    pub(crate) fn sanitize(&mut self) {
        if !self.volume.is_finite() {
            self.volume = Self::default().volume;
        }
        self.volume = self.volume.clamp(0.0, 1.0);
        self.tick_ms = self.tick_ms.max(1);
    }
}

/// Terminal adapter configuration.
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TuiConfig {
    /// Capture mouse events.
    pub mouse: bool,
    /// Retained for configuration compatibility.
    ///
    /// The built-in terminal loop is event-driven, so this value no longer
    /// controls latency; it is still validated and passed to
    /// [`ferriso_tui::Options`].
    pub poll_ms: u64,
}

impl Default for TuiConfig {
    fn default() -> Self {
        Self {
            mouse: false,
            poll_ms: 50,
        }
    }
}

impl TuiConfig {
    /// Options for `ferriso_tui::run`.
    pub fn options(&self) -> ferriso_tui::Options {
        ferriso_tui::Options {
            mouse: self.mouse,
            poll_interval: Duration::from_millis(self.poll_ms.max(1)),
        }
    }

    pub(crate) fn sanitize(&mut self) {
        self.poll_ms = self.poll_ms.max(1);
    }
}

/// Iced window configuration.
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GuiConfig {
    /// Window title.
    pub title: String,
    /// Window width in logical pixels.
    pub width: f32,
    /// Window height in logical pixels.
    pub height: f32,
}

impl Default for GuiConfig {
    fn default() -> Self {
        Self {
            title: DEFAULT_TITLE.into(),
            width: 1100.0,
            height: 760.0,
        }
    }
}

impl GuiConfig {
    /// Settings for `ferriso_gui::run`.
    pub fn settings(&self) -> ferriso_gui::Settings {
        ferriso_gui::Settings {
            title: self.title.clone(),
            size: (self.width, self.height),
            application_id: ferriso_gui::APPLICATION_ID.into(),
            ..ferriso_gui::Settings::default()
        }
    }

    pub(crate) fn sanitize(&mut self) {
        if self.title.trim().is_empty() {
            self.title = DEFAULT_TITLE.into();
        }
        self.width = sanitize_extent(self.width, Self::default().width, MIN_WIDTH);
        self.height = sanitize_extent(self.height, Self::default().height, MIN_HEIGHT);
    }
}

const MIN_WIDTH: f32 = 320.0;
const MIN_HEIGHT: f32 = 240.0;

fn sanitize_extent(value: f32, fallback: f32, min: f32) -> f32 {
    if !value.is_finite() || value <= 0.0 {
        fallback
    } else {
        value.max(min)
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "ferriso-player-config-{tag}-{}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("create temp dir");
            Self(path)
        }

        fn file(&self, body: &str) -> PathBuf {
            let path = self.0.join("config.toml");
            fs::write(&path, body).expect("write config");
            path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn load(path: &PathBuf) -> AppConfig {
        ferriso_config::Loader::new("ferriso-player-config-test")
            .local(None::<PathBuf>)
            .explicit(path)
            .load::<AppConfig>()
            .expect("load config")
            .sanitized()
    }

    #[test]
    fn default_config_matches_the_adapters_and_is_valid() {
        let config = AppConfig::default();
        assert_eq!(config.player.volume, 0.7);
        assert_eq!(config.player.tick(), Duration::from_millis(100));
        assert!(!config.tui.options().mouse);
        assert_eq!(
            config.tui.options().poll_interval,
            Duration::from_millis(50)
        );
        let settings = config.gui.settings();
        assert_eq!(settings.title, DEFAULT_TITLE);
        assert_eq!(settings.size, (1100.0, 760.0));
    }

    #[test]
    fn partial_sections_keep_defaults_and_values_are_normalized() {
        let dir = TempDir::new("partial");
        let path = dir.file(
            "[player]\nvolume = 5.0\n[tui]\nmouse = true\n[gui]\nwidth = 10.0\nheight = 900.0\n",
        );

        let config = load(&path);

        assert_eq!(config.player.volume, 1.0);
        assert_eq!(config.player.tick_ms, 100);
        assert!(config.tui.options().mouse);
        assert_eq!(
            config.tui.options().poll_interval,
            Duration::from_millis(50)
        );
        assert_eq!(config.gui.title, DEFAULT_TITLE);
        assert_eq!(config.gui.settings().size, (MIN_WIDTH, 900.0));
    }

    #[test]
    fn zero_poll_interval_is_repaired() {
        let dir = TempDir::new("zero");
        let path = dir.file("[tui]\npoll_ms = 0\n[player]\ntick_ms = 0\n");
        let config = load(&path);
        assert_eq!(config.tui.options().poll_interval, Duration::from_millis(1));
        assert_eq!(config.player.tick(), Duration::from_millis(1));
    }

    #[test]
    fn unknown_keys_are_rejected() {
        let dir = TempDir::new("unknown");
        let path = dir.file("[player]\nvolum = 0.5\n");
        assert!(
            ferriso_config::Loader::new("ferriso-player-config-test")
                .local(None::<PathBuf>)
                .explicit(path)
                .load::<AppConfig>()
                .is_err()
        );
    }
}
