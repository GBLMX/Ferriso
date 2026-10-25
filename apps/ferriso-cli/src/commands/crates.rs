use clawless::prelude::*;

/// Print the framework's crate map.
#[derive(Debug, Args)]
pub struct CratesArgs {}

/// List the framework crates, their dependency boundary, and their role.
#[command]
pub async fn crates(_args: CratesArgs, context: Context) -> CommandResult {
    artifact!(crate::catalog::CrateList {
        crates: crate::catalog::CRATES,
    });
    Ok(())
}
