//! Static framework inventory used by the `crates` and `apps` commands.
//!
//! Keeping the inventory in one place makes the CLI a useful map of the
//! workspace without linking the CLI to every application crate. The list
//! types implement `Display` for human output and `Serialize` for `--json`.

use std::fmt;

use serde::Serialize;

/// Minimum supported Rust version, mirrored from the workspace manifest.
pub(crate) const MSRV: (u64, u64) = (1, 88);

/// Human-readable form of [`MSRV`].
pub(crate) const MSRV_LABEL: &str = "1.88";

/// One published framework crate.
#[derive(Debug, Serialize)]
pub(crate) struct CrateInfo {
    pub(crate) name: &'static str,
    pub(crate) dependencies: &'static str,
    pub(crate) role: &'static str,
}

/// One workspace application.
#[derive(Debug, Serialize)]
pub(crate) struct AppInfo {
    pub(crate) name: &'static str,
    pub(crate) package: &'static str,
    pub(crate) frontends: &'static str,
    pub(crate) role: &'static str,
}

/// The `crates` command's human/JSON payload.
#[derive(Debug, Serialize)]
pub(crate) struct CrateList {
    pub(crate) crates: &'static [CrateInfo],
}

/// The `apps` command's human/JSON payload.
#[derive(Debug, Serialize)]
pub(crate) struct AppList {
    pub(crate) apps: &'static [AppInfo],
}

impl fmt::Display for CrateList {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for entry in self.crates {
            writeln!(
                formatter,
                "{:<22} {:<30} {}",
                entry.name, entry.dependencies, entry.role
            )?;
        }
        Ok(())
    }
}

impl fmt::Display for AppList {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for entry in self.apps {
            writeln!(
                formatter,
                "{:<14} {:<24} {:<10} {}",
                entry.name, entry.package, entry.frontends, entry.role
            )?;
        }
        Ok(())
    }
}

pub(crate) const CRATES: &[CrateInfo] = &[
    CrateInfo {
        name: "ferriso-core",
        dependencies: "Tokio",
        role: "actor runtime, App contract, commands, subscriptions, snapshots",
    },
    CrateInfo {
        name: "ferriso-diagnostics",
        dependencies: "thiserror, miette",
        role: "error contract, bounded event rings, report export",
    },
    CrateInfo {
        name: "ferriso-keymap",
        dependencies: "std",
        role: "frontend-neutral chords and binding maps",
    },
    CrateInfo {
        name: "ferriso-config",
        dependencies: "config, serde",
        role: "layered TOML and environment configuration",
    },
    CrateInfo {
        name: "ferriso-platform",
        dependencies: "std; native features opt in",
        role: "opener, clipboard, notifications, dialogs, file watching",
    },
    CrateInfo {
        name: "ferriso-test",
        dependencies: "ferriso-core",
        role: "headless actor harness",
    },
    CrateInfo {
        name: "ferriso-tracing",
        dependencies: "ferriso-diagnostics, tracing",
        role: "tracing backend for diagnostics records",
    },
    CrateInfo {
        name: "ferriso-tui",
        dependencies: "ratatui, crossterm",
        role: "terminal renderer and event loop",
    },
    CrateInfo {
        name: "ferriso-gui",
        dependencies: "iced",
        role: "iced renderer and window lifecycle",
    },
    CrateInfo {
        name: "ferriso-workspace",
        dependencies: "std",
        role: "TUI/iced-neutral tabs, split panes, table state",
    },
    CrateInfo {
        name: "ferriso-floem",
        dependencies: "Floem",
        role: "Floem boundary probe",
    },
    CrateInfo {
        name: "ferriso-gpui",
        dependencies: "GPUI",
        role: "GPUI boundary probe",
    },
    CrateInfo {
        name: "ferriso-editor",
        dependencies: "lsh, unicode-*",
        role: "editor core: gap buffer, undo, wrap, highlighting",
    },
    CrateInfo {
        name: "lsh",
        dependencies: "std",
        role: "vendored LSH syntax-highlighting runtime",
    },
    CrateInfo {
        name: "ferriso-cli",
        dependencies: "clawless",
        role: "this CLI: catalog, toolchain doctor, version",
    },
];

pub(crate) const APPS: &[AppInfo] = &[
    AppInfo {
        name: "player",
        package: "ferriso-player",
        frontends: "TUI + iced",
        role: "music-player practice app with one actor-owned view model",
    },
    AppInfo {
        name: "editor",
        package: "ferriso-editor-app",
        frontends: "TUI + iced",
        role: "tabbed editor with native ratatui and iced canvases",
    },
    AppInfo {
        name: "yazi-desktop",
        package: "ferriso-yazi-desktop",
        frontends: "TUI + iced",
        role: "file manager with layered config and preview panes",
    },
    AppInfo {
        name: "twozero48",
        package: "ferriso-twozero48",
        frontends: "iced",
        role: "animated 2048 probe for the GUI adapter",
    },
];

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn catalog_names_are_unique() {
        let crates: HashSet<_> = CRATES.iter().map(|entry| entry.name).collect();
        assert_eq!(crates.len(), CRATES.len());

        let apps: HashSet<_> = APPS.iter().map(|entry| entry.name).collect();
        assert_eq!(apps.len(), APPS.len());
    }

    #[test]
    fn every_app_declares_a_frontend_and_package() {
        for app in APPS {
            assert!(!app.frontends.is_empty(), "{} has no frontend", app.name);
            assert!(!app.package.is_empty(), "{} has no package", app.name);
        }
    }
}
