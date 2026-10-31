//! CLI command groups; domain behavior belongs to the backend.

use std::path::PathBuf;

use anyhow::Result;

pub mod add;
pub mod comment;
pub mod commit;
pub mod push;
pub mod review;
pub mod ui;

/// Resolve the canonical `.git` directory for the current working checkout without mutating filesystem state.
pub fn resolve_git_dir() -> Result<String> {
    let cwd = std::fs::canonicalize(std::env::current_dir()?)?;
    let ceilings: Vec<PathBuf> = std::env::var_os("GIT_CEILING_DIRECTORIES")
        .map(|raw| {
            std::env::split_paths(&raw)
                .map(|p| std::fs::canonicalize(&p).unwrap_or(p))
                .collect()
        })
        .unwrap_or_default();

    let mut cursor = Some(cwd.as_path());
    let mut found: Option<PathBuf> = None;
    while let Some(dir) = cursor {
        if dir != cwd && ceilings.iter().any(|c| c == dir) {
            break;
        }
        if dir.file_name().is_some_and(|name| name == ".git") || dir.join(".git").exists() {
            found = Some(dir.to_path_buf());
            break;
        }
        if ceilings.iter().any(|c| c == dir) {
            break;
        }
        cursor = dir.parent();
    }
    let Some(repo_root) = found else {
        return Err(crate::backend::error::Error::Invalid(format!(
            "not a git repository (or any of the parent directories): '{}'",
            cwd.display()
        ))
        .into());
    };
    let (_, _, normalized_git_dir) =
        crate::backend::utils::resolve_git_paths(&repo_root.to_string_lossy())?;
    Ok(normalized_git_dir)
}
