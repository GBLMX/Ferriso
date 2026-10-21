//! One-stop layered configuration for Ferriso, built on [`config`].
//!
//! Ferriso reads configuration from a fixed stack of sources and overlays them
//! from lowest to highest precedence:
//!
//! | # | Source | Location |
//! | --- | --- | --- |
//! | 1 | Built-in defaults | `#[serde(default)]` on the application's config type |
//! | 2 | System file | `/etc/ferriso/config.toml` (Linux), `/Library/Application Support/ferriso/config.toml` (macOS), `%PROGRAMDATA%\ferriso\config.toml` (Windows) |
//! | 3 | User file | `$XDG_CONFIG_HOME/ferriso/config.toml`, else `~/.config/ferriso/config.toml` (`%APPDATA%` on Windows) |
//! | 4 | Local file | `./ferriso.toml` |
//! | 5 | Explicit file | `$FERRISO_CONFIG` |
//! | 6 | Environment | `FERRISO__GUI__WIDTH=1400`, `FERRISO__PLAYER__VOLUME=0.25`, ... |
//!
//! Layers 2-4 are optional: a missing file is simply skipped. Layer 5 is
//! required, so a bad `$FERRISO_CONFIG` fails loudly instead of silently
//! turning into defaults. All files are TOML.
//!
//! Load a typed snapshot with [`load`]:
//!
//! ```no_run
//! # use serde::Deserialize;
//! #[derive(Deserialize, Default)]
//! #[serde(default)]
//! struct Config {
//!     gui: Gui,
//! }
//!
//! #[derive(Deserialize)]
//! #[serde(default)]
//! struct Gui {
//!     width: f64,
//! }
//!
//! impl Default for Gui {
//!     fn default() -> Self {
//!         Self { width: 1100.0 }
//!     }
//! }
//!
//! # fn main() -> Result<(), ferriso_config::ConfigError> {
//! let config: Config = ferriso_config::load()?;
//! println!("{}", config.gui.width);
//! # Ok(())
//! # }
//! ```
//!
//! Deserialization uses serde's normal field defaults, so a config file may set
//! only the keys it cares about; everything else comes from the built-in
//! defaults.

use std::path::{Path, PathBuf};

use config::{Config, Environment, File, FileFormat};
use serde::de::DeserializeOwned;

/// Application name used for conventional file locations and the environment
/// prefix.
pub const APP: &str = "ferriso";

/// Separator between nested keys in environment variables.
pub const ENV_SEPARATOR: &str = "__";

/// Configuration failure with a stable code, an actionable help message, and
/// the original `config` error preserved as its source.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum ConfigError {
    #[error("explicit configuration file {path} does not exist")]
    #[diagnostic(
        code(ferriso::config::missing),
        help("Create {path}, unset {env_var}, or point {env_var} at an existing TOML file.")
    )]
    MissingExplicit { path: String, env_var: String },

    #[error("configuration is invalid; searched: {layers}")]
    #[diagnostic(code(ferriso::config::invalid), help("{hint}"))]
    Invalid {
        layers: String,
        hint: String,
        #[source]
        source: Box<config::ConfigError>,
    },
}

/// One configuration layer as it exists on disk right now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceInfo {
    pub label: &'static str,
    pub path: PathBuf,
    pub required: bool,
    pub exists: bool,
}

impl std::fmt::Display for SourceInfo {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{}={} ({})",
            self.label,
            self.path.display(),
            if self.exists {
                "present"
            } else if self.required {
                "required, missing"
            } else {
                "optional, missing"
            }
        )
    }
}

/// Layered configuration loader.
///
/// Construct with [`Loader::new`], optionally override discovery, then call
/// [`Loader::load`].
#[derive(Clone, Debug)]
pub struct Loader {
    app: String,
    env_prefix: String,
    local: Option<PathBuf>,
    explicit: Option<PathBuf>,
}

impl Loader {
    /// Prepare the conventional Ferriso layers for `app`.
    ///
    /// The application name determines the file locations
    /// (`/etc/<app>/config.toml`, `~/.config/<app>/config.toml`,
    /// `./<app>.toml`), the explicit-file variable (`<APP>_CONFIG`) and the
    /// environment prefix (`<APP>__KEY__SUBKEY`).
    #[must_use]
    pub fn new(app: impl Into<String>) -> Self {
        let app = app.into();
        let env_prefix = app.to_uppercase().replace(['-', '.'], "_");
        Self {
            local: Some(PathBuf::from(format!("{app}.toml"))),
            app,
            env_prefix,
            explicit: None,
        }
    }

    /// Replace the project-local file. `None` disables the layer.
    #[must_use]
    pub fn local(mut self, path: Option<impl Into<PathBuf>>) -> Self {
        self.local = path.map(Into::into);
        self
    }

    /// Add a required explicit file. Without this, `$<APP>_CONFIG` is consulted.
    #[must_use]
    pub fn explicit(mut self, path: impl Into<PathBuf>) -> Self {
        self.explicit = Some(path.into());
        self
    }

    /// Read every layer and deserialize the merged result.
    pub fn load<T: DeserializeOwned>(self) -> Result<T, ConfigError> {
        let sources = self.sources();
        let env_var = format!("{}_CONFIG", self.env_prefix);

        if let Some(explicit) = self.explicit_path()
            && !explicit.is_file()
        {
            return Err(ConfigError::MissingExplicit {
                path: explicit.display().to_string(),
                env_var,
            });
        }

        let config = self
            .build()
            .map_err(|source| invalid_configuration(source, &sources))?;
        config
            .try_deserialize()
            .map_err(|source| invalid_configuration(source, &sources))
    }

    /// Every candidate layer, its precedence position, and whether it exists.
    ///
    /// This is safe to include in diagnostics: paths only, never file contents.
    pub fn sources(&self) -> Vec<SourceInfo> {
        let mut sources = vec![SourceInfo {
            label: "system",
            path: system_file(&self.app),
            required: false,
            exists: system_file(&self.app).is_file(),
        }];

        if let Some(user) = user_file(&self.app) {
            sources.push(SourceInfo {
                label: "user",
                path: user.clone(),
                required: false,
                exists: user.is_file(),
            });
        }
        if let Some(local) = &self.local {
            sources.push(SourceInfo {
                label: "local",
                path: local.clone(),
                required: false,
                exists: local.is_file(),
            });
        }
        if let Some(explicit) = self.explicit_path() {
            sources.push(SourceInfo {
                label: "explicit",
                path: explicit.clone(),
                required: true,
                exists: explicit.is_file(),
            });
        }

        sources
    }

    fn explicit_path(&self) -> Option<PathBuf> {
        self.explicit
            .clone()
            .or_else(|| env_path(&format!("{}_CONFIG", self.env_prefix)))
    }

    fn build(self) -> Result<Config, config::ConfigError> {
        let explicit = self.explicit_path();

        let mut builder = Config::builder().add_source(toml_file(&system_file(&self.app), false));

        if let Some(user) = user_file(&self.app) {
            builder = builder.add_source(toml_file(&user, false));
        }
        if let Some(local) = &self.local {
            builder = builder.add_source(toml_file(local, false));
        }
        if let Some(explicit) = &explicit {
            builder = builder.add_source(toml_file(explicit, true));
        }

        builder
            .add_source(
                Environment::with_prefix(&self.env_prefix)
                    .separator(ENV_SEPARATOR)
                    .try_parsing(true)
                    .ignore_empty(true),
            )
            .build()
    }
}

/// Load the conventional Ferriso configuration.
pub fn load<T: DeserializeOwned>() -> Result<T, ConfigError> {
    Loader::new(APP).load()
}

fn invalid_configuration(source: config::ConfigError, sources: &[SourceInfo]) -> ConfigError {
    let layers = sources
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    ConfigError::Invalid {
        layers,
        hint: hint_for(&source),
        source: Box::new(source),
    }
}

fn hint_for(source: &config::ConfigError) -> String {
    let message = source.to_string().to_ascii_lowercase();

    if message.contains("unknown field") {
        "Remove or rename the key named in the error. Unknown keys are rejected so typos cannot be silently ignored."
            .into()
    } else if message.contains("invalid type") {
        "Use the type reported by the error: string, integer, float, boolean, list, or table. Environment values are parsed from text."
            .into()
    } else if message.contains("missing field") {
        "Add the required key named in the error. Keys with built-in defaults may be omitted."
            .into()
    } else if message.contains("expected") || message.contains("parse") || message.contains("toml")
    {
        "Fix the TOML syntax at the reported line and column, then retry.".into()
    } else if message.contains("environment") {
        "Check the environment variable named in the error. Nested keys use `__`, for example APP__GUI__WIDTH=1400."
            .into()
    } else {
        "Compare the named layer with the documented schema, or remove that layer so built-in defaults apply."
            .into()
    }
}

fn toml_file(path: &Path, required: bool) -> File<config::FileSourceFile, FileFormat> {
    File::from(path.to_path_buf())
        .format(FileFormat::Toml)
        .required(required)
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn system_file(app: &str) -> PathBuf {
    if cfg!(windows) {
        env_path("PROGRAMDATA")
            .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
            .join(app)
            .join("config.toml")
    } else if cfg!(target_os = "macos") {
        PathBuf::from("/Library/Application Support")
            .join(app)
            .join("config.toml")
    } else {
        PathBuf::from("/etc").join(app).join("config.toml")
    }
}

fn user_file(app: &str) -> Option<PathBuf> {
    let base = if cfg!(windows) {
        env_path("APPDATA")?
    } else if cfg!(target_os = "macos") {
        env_path("HOME")?.join("Library/Application Support")
    } else {
        env_path("XDG_CONFIG_HOME").or_else(|| env_path("HOME").map(|home| home.join(".config")))?
    };
    Some(base.join(app).join("config.toml"))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use miette::Diagnostic as _;
    use serde::Deserialize;

    use super::*;

    #[derive(Debug, Default, PartialEq, Deserialize)]
    #[serde(default)]
    struct Fixture {
        gui: Gui,
        player: Player,
    }

    #[derive(Debug, PartialEq, Deserialize)]
    #[serde(default)]
    struct Gui {
        title: String,
        width: f64,
    }

    #[derive(Debug, PartialEq, Deserialize, Default)]
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

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("ferriso-config-{tag}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("create temp dir");
            Self(path)
        }

        fn file(&self, name: &str, body: &str) -> PathBuf {
            let path = self.0.join(name);
            fs::write(&path, body).expect("write config");
            path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn loader(tag: &str) -> Loader {
        // The tag also isolates the environment prefix, so a developer's own
        // FERRISO__* variables cannot change the outcome.
        Loader::new(format!("ferriso-config-{tag}")).local(None::<PathBuf>)
    }

    #[test]
    fn defaults_apply_when_every_optional_file_is_missing() {
        let config: Fixture = loader("defaults").load().expect("load defaults");
        assert_eq!(config, Fixture::default());
    }

    #[test]
    fn explicit_file_overrides_local_file() {
        let dir = TempDir::new("precedence");
        let local = dir.file("local.toml", "[gui]\nwidth = 200.0\ntitle = \"local\"\n");
        let explicit = dir.file(
            "explicit.toml",
            "[gui]\nwidth = 300.0\n[player]\nvolume = 0.25\n",
        );

        let config: Fixture = Loader::new("ferriso-config-precedence")
            .local(Some(local))
            .explicit(&explicit)
            .load()
            .expect("load layers");

        assert_eq!(config.gui.width, 300.0);
        assert_eq!(config.gui.title, "local");
        assert_eq!(config.player.volume, 0.25);
        assert_eq!(config.player.tick_ms, 0);
    }

    #[test]
    fn explicit_file_is_required() {
        let dir = TempDir::new("required");
        let missing = dir.0.join("absent.toml");
        let error = loader("required")
            .explicit(&missing)
            .load::<Fixture>()
            .expect_err("missing explicit file must fail");
        assert!(
            error.to_string().contains("absent.toml"),
            "error should name the file: {error}"
        );
        assert!(matches!(error, ConfigError::MissingExplicit { .. }));
        let help = error.help().expect("actionable help").to_string();
        assert!(help.contains("FERRISO_CONFIG_REQUIRED_CONFIG"), "{help}");
        assert!(help.contains("absent.toml"), "{help}");
    }

    #[test]
    fn unknown_keys_explain_how_to_fix_them() {
        #[derive(Debug, Default, PartialEq, Deserialize)]
        #[serde(default, deny_unknown_fields)]
        struct Strict {
            value: u8,
        }

        let dir = TempDir::new("unknown-key");
        let path = dir.file("strict.toml", "value = 1\nstray = true\n");
        let error = loader("unknown-key")
            .explicit(&path)
            .load::<Strict>()
            .expect_err("unknown key must fail");

        assert_eq!(
            error.code().map(|code| code.to_string()).as_deref(),
            Some("ferriso::config::invalid")
        );
        let help = error.help().expect("hint").to_string();
        assert!(help.contains("Unknown keys are rejected"), "{help}");
        assert!(
            std::error::Error::source(&error)
                .map(ToString::to_string)
                .is_some_and(|source| source.contains("unknown field")),
            "config source must be preserved"
        );
    }

    #[test]
    fn sources_report_paths_and_required_state_without_values() {
        let dir = TempDir::new("sources");
        let local = dir.file("local.toml", "[gui]\ntitle = \"secret\"\n");
        let explicit = dir.0.join("missing.toml");
        let sources = Loader::new("ferriso-config-sources")
            .local(Some(local.clone()))
            .explicit(&explicit)
            .sources();

        let local_info = sources
            .iter()
            .find(|source| source.label == "local")
            .expect("local layer");
        assert_eq!(local_info.path, local);
        assert!(local_info.exists);
        assert!(!local_info.required);

        let explicit_info = sources
            .iter()
            .find(|source| source.label == "explicit")
            .expect("explicit layer");
        assert!(explicit_info.required);
        assert!(!explicit_info.exists);
        assert!(!explicit_info.to_string().contains("secret"));
    }

    #[test]
    fn malformed_file_is_an_error() {
        let dir = TempDir::new("malformed");
        let path = dir.file("broken.toml", "[gui\nwidth = ");
        assert!(
            loader("malformed")
                .explicit(path)
                .load::<Fixture>()
                .is_err()
        );
    }

    #[test]
    fn application_name_derives_the_environment_prefix() {
        let loader = Loader::new("ferriso-config-prefix");
        assert_eq!(loader.env_prefix, "FERRISO_CONFIG_PREFIX");
        assert_eq!(
            loader.local,
            Some(PathBuf::from("ferriso-config-prefix.toml"))
        );
    }
}
