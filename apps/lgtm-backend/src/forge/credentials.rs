//! Read local gh credentials for the explicitly supported GitHub host/account.
use crate::error::{Result, invalid};
use std::time::Duration;

/// Captures stdout privately; gh diagnostics may contain account details and are not forwarded.
pub async fn get_token(account: Option<&str>) -> Result<String> {
    if account.is_some_and(|v| v.is_empty() || v.starts_with('-') || v.contains(['\n', '\r'])) {
        return Err(invalid("Invalid GitHub account"));
    }
    let mut command = tokio::process::Command::new("gh");
    command.args(["auth", "token", "--hostname", "github.com"]);
    if let Some(account) = account {
        // Explicit account selection must not silently use an environment token.
        command.args(["--user", account]);
        command.env_remove("GH_TOKEN").env_remove("GITHUB_TOKEN");
    }
    command.kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(10), command.output())
        .await
        .map_err(|_| invalid("GitHub credential lookup timed out"))?
        .map_err(|_| invalid("GitHub requires gh installed and authenticated"))?;
    if !output.status.success() {
        return Err(invalid(
            "GitHub credentials unavailable; run gh auth login for the selected account",
        ));
    }
    let token = String::from_utf8(output.stdout)
        .map_err(|_| invalid("Invalid GitHub credential encoding"))?;
    let token = token.trim();
    if token.is_empty() || token.contains(['\n', '\r']) {
        return Err(invalid("GitHub credential lookup returned no usable token"));
    }
    Ok(token.to_owned())
}
