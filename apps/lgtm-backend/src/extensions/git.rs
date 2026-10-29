//! Read-only Git commands shared by saved-source and source-presence queries.
use crate::error::{Result, invalid};
use std::path::Path;

/// Run an owned-store mutation with explicit environment and optional binary input.
pub fn run(
    root: &Path,
    args: &[&str],
    env: &[(&str, &std::ffi::OsStr)],
    input: Option<&[u8]>,
) -> Result<Vec<u8>> {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let mut command = Command::new("git");
    command
        .arg("--literal-pathspecs")
        .arg("-C")
        .arg(root)
        .args(args)
        .envs(env.iter().copied())
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn()?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| invalid("Git input unavailable"))?;
    let bytes = input.unwrap_or_default().to_vec();
    let writer = std::thread::spawn(move || stdin.write_all(&bytes));
    let output = child.wait_with_output()?;
    let written = writer.join().map_err(|_| invalid("Git input failed"))?;
    if !output.status.success() {
        return Err(invalid(format!(
            "Snapshot Git operation failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    written?;
    Ok(output.stdout)
}

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
