use std::fmt;

use clawless::prelude::*;
use serde::Serialize;

/// Print the Ferriso CLI and workspace MSRV versions.
#[derive(Debug, Args)]
pub struct VersionArgs {}

#[derive(Debug, Serialize)]
struct VersionInfo {
    version: &'static str,
    msrv: &'static str,
}

impl fmt::Display for VersionInfo {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "ferriso {} (MSRV {})", self.version, self.msrv)
    }
}

/// Print version information for the framework CLI.
#[command]
pub async fn version(_args: VersionArgs, context: Context) -> CommandResult {
    artifact!(VersionInfo {
        version: env!("CARGO_PKG_VERSION"),
        msrv: crate::catalog::MSRV_LABEL,
    });
    Ok(())
}
