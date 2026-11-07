//! Desktop workspace launcher command (`lgtm open [path]` and `lgtm [path]`).

use std::{
    path::PathBuf,
    process::{Command, Stdio},
};

use anyhow::Context;
use clap::Args;
use lgtm_config::{ConfigService, LGTM_WORKSPACE_ENV, LgtmConfigError};
use lgtm_forge::error::Error as ForgeError;

pub use crate::models::OpenCommandOutput;

/// Arguments for the `lgtm open` command.
#[derive(Args, Debug, Clone, Default)]
pub struct OpenArgs {
    /// Directory to open in LGTM Desktop (defaults to current working directory).
    pub path: Option<PathBuf>,
}

/// Resolves the target workspace directory, locates the `lgtm-desktop` executable, and spawns it detached.
#[tracing::instrument(skip_all, fields(path = ?args.path))]
pub fn run(args: OpenArgs, json: bool) -> anyhow::Result<()> {
    let workspace_dir =
        ConfigService::resolve_workspace_dir(args.path.as_deref()).map_err(map_config_error)?;
    let executable = ConfigService::resolve_desktop_executable().map_err(map_config_error)?;
    let workspace_path = workspace_dir.to_string_lossy().to_string();

    crate::daemon::disable_stdio_inheritance();
    Command::new(&executable)
        .arg(&workspace_path)
        .env(LGTM_WORKSPACE_ENV, &workspace_path)
        .current_dir(&workspace_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("Launch LGTM Desktop ({})", executable.display()))?;

    OpenCommandOutput {
        workspace_path,
        executable: executable.to_string_lossy().to_string(),
    }
    .print(json);
    Ok(())
}

// Private ---------------------------------------------------------------------

fn map_config_error(err: LgtmConfigError) -> anyhow::Error {
    match err {
        LgtmConfigError::WorkspaceDirectoryNotFound { .. }
        | LgtmConfigError::WorkspacePathNotDirectory { .. } => {
            ForgeError::Invalid(err.to_string()).into()
        }
        LgtmConfigError::DesktopExecutableNotFound { detail } => {
            ForgeError::NotFound(detail).into()
        }
        other => other.into(),
    }
}
