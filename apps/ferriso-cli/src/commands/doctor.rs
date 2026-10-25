use std::{fmt, process::Command};

use clawless::prelude::*;
use serde::Serialize;

/// Check the local toolchain against Ferriso's MSRV.
#[derive(Debug, Args)]
pub struct DoctorArgs {}

#[derive(Debug, Serialize)]
struct ToolchainReport {
    rustc: String,
    cargo: String,
    msrv: &'static str,
    satisfied: bool,
}

impl fmt::Display for ToolchainReport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(formatter, "rustc: {}", self.rustc)?;
        writeln!(formatter, "cargo: {}", self.cargo)?;
        if self.satisfied {
            write!(formatter, "toolchain satisfies Ferriso MSRV {}", self.msrv)
        } else {
            write!(
                formatter,
                "toolchain is older than Ferriso MSRV {}",
                self.msrv
            )
        }
    }
}

/// Report the Rust toolchain and fail when it is older than the workspace MSRV.
#[command]
pub async fn doctor(_args: DoctorArgs, context: Context) -> CommandResult {
    let rustc = tool_version("rustc")?;
    let cargo = tool_version("cargo")?;
    let version = parse_version(&rustc)
        .ok_or_else(|| Error::msg(format!("could not parse rustc version from `{rustc}`")))?;
    let satisfied = version >= crate::catalog::MSRV;

    artifact!(ToolchainReport {
        rustc,
        cargo,
        msrv: crate::catalog::MSRV_LABEL,
        satisfied,
    });

    if !satisfied {
        return Err(Error::msg(format!(
            "Rust {}.{} is older than Ferriso's MSRV {}",
            version.0,
            version.1,
            crate::catalog::MSRV_LABEL
        )));
    }
    Ok(())
}

fn tool_version(tool: &str) -> CommandResult<String> {
    let output = Command::new(tool)
        .arg("--version")
        .output()
        .with_context(|| format!("run `{tool} --version`"))?;
    if !output.status.success() {
        return Err(Error::msg(format!(
            "`{tool} --version` exited with {}",
            output.status
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn parse_version(version: &str) -> Option<(u64, u64)> {
    let token = version.split_whitespace().nth(1)?;
    let mut parts = token.split('.');
    Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::parse_version;

    #[test]
    fn parses_stable_and_nightly_versions() {
        assert_eq!(parse_version("rustc 1.88.0 (abc)"), Some((1, 88)));
        assert_eq!(parse_version("rustc 1.93.0-nightly (abc)"), Some((1, 93)));
        assert_eq!(parse_version("rustc"), None);
    }
}
