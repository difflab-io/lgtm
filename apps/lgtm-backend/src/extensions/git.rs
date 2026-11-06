//! Installed Git process adapter. Snapshot policy belongs to the snapshot service.
use crate::error::{Result, invalid};
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

pub fn command(root: &Path) -> Command {
    let mut command = Command::new("git");
    for name in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_COMMON_DIR",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    ] {
        command.env_remove(name);
    }
    command
        .args([
            "--no-optional-locks",
            "-c",
            "core.hooksPath=/dev/null",
            "-C",
        ])
        .arg(root);
    command.env("GIT_TERMINAL_PROMPT", "0");
    command
}

pub fn run(mut command: Command, input: Option<&[u8]>) -> Result<Vec<u8>> {
    command
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn()?;
    if let Some(input) = input {
        let result = child
            .stdin
            .take()
            .ok_or_else(|| invalid("Git stdin missing"))?
            .write_all(input);
        if let Err(error) = result {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error.into());
        }
    }
    let output = child.wait_with_output()?;
    if !output.status.success() {
        tracing::error!(error = %String::from_utf8_lossy(&output.stderr), "Snapshot Git operation failed");
        return Err(invalid("Local snapshot Git operation failed"));
    }
    Ok(output.stdout)
}
