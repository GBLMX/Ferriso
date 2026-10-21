//! Layered configuration for yazi-desktop.

use std::{
    env,
    path::{Path, PathBuf},
};

use ferriso_config::SourceInfo;
use serde::Deserialize;

/// Desktop identity, matching `packaging/io.github.gblmx.yazi-desktop.desktop`.
pub const APPLICATION_ID: &str = "io.github.gblmx.yazi-desktop";

/// Layers/files/environment are namespaced under this application name.
pub const APP: &str = "yazi-desktop";

/// Default window title.
pub const DEFAULT_TITLE: &str = "yazi-desktop";

/// Entire layered configuration.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub start: Start,
    pub window: Window,
    pub art: Art,
    pub debug: DebugOptions,
}

impl Config {
    /// Load every layer through `ferriso-config` and normalize the values.
    pub fn load() -> Result<Self, ferriso_config::ConfigError> {
        Self::load_detailed().map(|(config, _sources)| config)
    }

    /// Load the configuration plus the layer list for diagnostics.
    ///
    /// The layer list carries paths and existence flags only, never values.
    pub fn load_detailed() -> Result<(Self, Vec<SourceInfo>), ferriso_config::ConfigError> {
        let loader = ferriso_config::Loader::new(APP);
        let sources = loader.sources();
        let config = loader.load::<Self>()?.sanitized();
        Ok((config, sources))
    }

    /// Secret-free configuration summary with every layer and its status.
    pub fn summary(&self, sources: &[SourceInfo]) -> String {
        let mut summary = format!(
            "start.path={}\nstart.show_hidden={}\nwindow.title={}\nwindow.width={}\nwindow.height={}\nart.enabled={}\nart.intensity={}\nart.speed={}\ndebug.enabled={}\ndebug.overlay={}\ndebug.stderr={}\nlayers:\n",
            self.start.path,
            self.start.show_hidden,
            self.window.title,
            self.window.width,
            self.window.height,
            self.art.enabled,
            self.art.intensity,
            self.art.speed,
            self.debug.enabled,
            self.debug.overlay,
            self.debug.stderr,
        );

        for source in sources {
            summary.push_str(&format!("  {source}\n"));
        }
        summary
    }

    #[must_use]
    pub fn sanitized(mut self) -> Self {
        self.art.intensity = if self.art.intensity.is_finite() {
            self.art.intensity.clamp(0.0, 1.0)
        } else {
            Art::default().intensity
        };
        self.art.speed = if self.art.speed.is_finite() {
            self.art.speed.clamp(0.0, 4.0)
        } else {
            Art::default().speed
        };
        self.window.width = dimension(self.window.width, Window::default().width, 640.0);
        self.window.height = dimension(self.window.height, Window::default().height, 420.0);
        if self.window.title.trim().is_empty() {
            self.window.title = DEFAULT_TITLE.into();
        }
        self
    }

    /// The initial directory, validated and with `~` expanded.
    pub fn start_dir(&self) -> PathBuf {
        let requested = expand_home(&self.start.path);
        if requested.is_dir() {
            requested
        } else {
            home().unwrap_or_else(|| PathBuf::from("/"))
        }
    }

    pub fn settings(&self) -> ferriso_gui::Settings {
        ferriso_gui::Settings {
            title: self.window.title.clone(),
            size: (self.window.width, self.window.height),
            application_id: APPLICATION_ID.into(),
        }
    }
}

/// Where the first listing comes from.
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Start {
    /// Initial directory. `~` expands to the user's home directory.
    pub path: String,
    /// Include dotfiles in the listing.
    pub show_hidden: bool,
}

impl Default for Start {
    fn default() -> Self {
        Self {
            path: "~".into(),
            show_hidden: false,
        }
    }
}

/// Window options.
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Window {
    pub title: String,
    pub width: f32,
    pub height: f32,
}

impl Default for Window {
    fn default() -> Self {
        Self {
            title: DEFAULT_TITLE.into(),
            width: 1240.0,
            height: 780.0,
        }
    }
}

/// Diagnostics options. Off in production; every switch is opt-in.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DebugOptions {
    /// Capture events and allow verbose logging.
    pub enabled: bool,
    /// Open the in-window event panel immediately.
    pub overlay: bool,
    /// Install the built-in stderr logger (a plugin example, not a dependency).
    pub stderr: bool,
}

/// Aurora shader options.
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Art {
    /// Draw the animated WGSL backdrop.
    pub enabled: bool,
    /// Ribbon strength, `0.0..=1.0`.
    pub intensity: f32,
    /// Animation speed multiplier, `0.0..=4.0`.
    pub speed: f32,
}

impl Default for Art {
    fn default() -> Self {
        Self {
            enabled: true,
            intensity: 0.85,
            speed: 1.0,
        }
    }
}

/// Home directory, from `$HOME`.
pub fn home() -> Option<PathBuf> {
    env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn expand_home(path: &str) -> PathBuf {
    if path == "~" {
        home().unwrap_or_else(|| PathBuf::from("/"))
    } else if let Some(rest) = path.strip_prefix("~/") {
        home()
            .map(|home| home.join(rest))
            .unwrap_or_else(|| PathBuf::from(path))
    } else {
        let path = Path::new(path);
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            env::current_dir()
                .map(|cwd| cwd.join(path))
                .unwrap_or_else(|_| path.to_path_buf())
        }
    }
}

fn dimension(value: f32, fallback: f32, min: f32) -> f32 {
    if !value.is_finite() || value <= 0.0 {
        fallback
    } else {
        value.max(min)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid() {
        let config = Config::default();
        assert!(config.art.enabled);
        assert_eq!(config.art.speed, 1.0);
        assert_eq!(config.settings().application_id, APPLICATION_ID);
        assert_eq!(config.settings().size, (1240.0, 780.0));
        assert!(!config.debug.enabled);
        assert!(!config.debug.overlay);
        assert!(!config.debug.stderr);
    }

    #[test]
    fn values_are_normalized() {
        let config = Config {
            start: Start {
                path: "~".into(),
                show_hidden: true,
            },
            window: Window {
                title: "  ".into(),
                width: 0.0,
                height: f32::NAN,
            },
            art: Art {
                enabled: true,
                intensity: 9.0,
                speed: -2.0,
            },
            debug: DebugOptions::default(),
        }
        .sanitized();

        assert_eq!(config.window.title, DEFAULT_TITLE);
        assert_eq!(config.window.width, 1240.0);
        assert_eq!(config.window.height, 780.0);
        assert_eq!(config.art.intensity, 1.0);
        assert_eq!(config.art.speed, 0.0);
    }
}
