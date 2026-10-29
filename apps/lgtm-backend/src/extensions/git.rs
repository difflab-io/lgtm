//! Read-only Git commands shared by saved-source and source-presence queries.
use crate::error::{Result, invalid};
use std::path::Path;

/// Literal paths and disabled optional locks keep source reads independent of index refreshes.
pub fn get_bytes(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let output = std::process::Command::new("git")
        .arg("--literal-pathspecs")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()?;
    if !output.status.success() {
        return Err(invalid("Selected Git source is unavailable"));
    }
    Ok(output.stdout)
}
