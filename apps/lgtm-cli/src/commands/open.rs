//! Desktop workspace launcher command (`lgtm open [path]` and `lgtm [path]`).

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::Context;
use clap::Args;
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
    let workspace_dir = resolve_workspace_dir(args.path.as_deref())?;
    let executable = resolve_desktop_executable()?;
    let workspace_path = workspace_dir.to_string_lossy().to_string();

    Command::new(&executable)
        .arg(&workspace_path)
        .env("LGTM_WORKSPACE", &workspace_path)
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

fn resolve_workspace_dir(path: Option<&Path>) -> anyhow::Result<PathBuf> {
    let target = path.unwrap_or_else(|| Path::new("."));
    if !target.exists() {
        return Err(ForgeError::Invalid(format!(
            "Workspace directory '{}' does not exist",
            target.display()
        ))
        .into());
    }
    if !target.is_dir() {
        return Err(ForgeError::Invalid(format!(
            "Workspace path '{}' is not a directory",
            target.display()
        ))
        .into());
    }
    fs::canonicalize(target)
        .with_context(|| format!("Canonicalize workspace directory '{}'", target.display()))
}

fn resolve_desktop_executable() -> anyhow::Result<PathBuf> {
    if let Some(configured) = std::env::var_os("LGTM_DESKTOP_BIN")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
    {
        if configured.is_file() {
            return Ok(fs::canonicalize(&configured).unwrap_or(configured));
        }
        if let Some(found) = find_in_path(&configured) {
            return Ok(found);
        }
        return Err(ForgeError::NotFound(format!(
            "Configured LGTM_DESKTOP_BIN '{}' does not exist",
            configured.display()
        ))
        .into());
    }

    let bin_name = desktop_binary_name();
    if let Ok(current_exe) = std::env::current_exe()
        && let Some(exe_dir) = current_exe.parent()
    {
        let sibling = exe_dir.join(bin_name);
        if sibling.is_file() {
            return Ok(fs::canonicalize(&sibling).unwrap_or(sibling));
        }
    }

    if let Some(found) = find_in_path(Path::new(bin_name)) {
        return Ok(found);
    }

    for candidate in macos_app_candidates() {
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    Err(ForgeError::NotFound(
        "Could not find lgtm-desktop executable; set LGTM_DESKTOP_BIN or install LGTM Desktop"
            .to_owned(),
    )
    .into())
}

fn desktop_binary_name() -> &'static str {
    if cfg!(windows) {
        "lgtm-desktop.exe"
    } else {
        "lgtm-desktop"
    }
}

fn find_in_path(binary: &Path) -> Option<PathBuf> {
    if binary.components().count() > 1 {
        return None;
    }
    let path_var = std::env::var_os("PATH")?;
    std::env::split_paths(&path_var)
        .map(|dir| dir.join(binary))
        .find(|candidate| candidate.is_file())
}

fn macos_app_candidates() -> Vec<PathBuf> {
    let mut candidates = vec![
        PathBuf::from("/Applications/lgtm-desktop.app/Contents/MacOS/lgtm-desktop"),
        PathBuf::from("/Applications/LGTM.app/Contents/MacOS/lgtm-desktop"),
    ];
    if let Some(home) = dirs::home_dir() {
        candidates.push(home.join("Applications/lgtm-desktop.app/Contents/MacOS/lgtm-desktop"));
        candidates.push(home.join("Applications/LGTM.app/Contents/MacOS/lgtm-desktop"));
    }
    candidates
}

// tests -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // tests -------------------------------------------------------------------

    mod given_a_workspace_path {
        use super::*;

        mod when_resolving_workspace_dir {
            use super::*;

            #[test]
            fn should_canonicalize_existing_directory() {
                // Arrange
                let temp = tempfile::tempdir().unwrap();
                let nested = temp.path().join("project");
                fs::create_dir_all(&nested).unwrap();

                // Act
                let resolved = resolve_workspace_dir(Some(&nested)).unwrap();

                // Assert
                assert_eq!(resolved, fs::canonicalize(&nested).unwrap());
            }

            #[test]
            fn should_reject_nonexistent_directory() {
                // Arrange
                let temp = tempfile::tempdir().unwrap();
                let missing = temp.path().join("missing");

                // Act
                let err = resolve_workspace_dir(Some(&missing)).unwrap_err();

                // Assert
                let forge_err = err.downcast_ref::<ForgeError>().unwrap();
                assert!(matches!(forge_err, ForgeError::Invalid(_)));
            }

            #[test]
            fn should_reject_file_path() {
                // Arrange
                let temp = tempfile::tempdir().unwrap();
                let file_path = temp.path().join("README.md");
                fs::write(&file_path, "# demo\n").unwrap();

                // Act
                let err = resolve_workspace_dir(Some(&file_path)).unwrap_err();

                // Assert
                let forge_err = err.downcast_ref::<ForgeError>().unwrap();
                assert!(matches!(forge_err, ForgeError::Invalid(_)));
            }
        }
    }
}
