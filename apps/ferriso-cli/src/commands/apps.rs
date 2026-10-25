use clawless::prelude::*;

/// Print the workspace application inventory.
#[derive(Debug, Args)]
pub struct AppsArgs {}

/// List the workspace apps and the frontends each one proves.
#[command]
pub async fn apps(_args: AppsArgs, context: Context) -> CommandResult {
    artifact!(crate::catalog::AppList {
        apps: crate::catalog::APPS,
    });
    Ok(())
}
