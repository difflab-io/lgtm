//! git2-backed implementation of the two review primitives:
//!
//! - [`changed_files`] — files changed between `base_ref` and the current working tree
//! - [`file_diff`] — unified diff for one file between `base_ref` and the working tree
//!
//! `base_ref` is resolved via `Repository::revparse_single`, so it accepts
//! branch names ("main"), tags, and commit SHAs. The working tree (including
//! unstaged edits) is always the "to" side; this mirrors how a reviewer
//! typically asks "what have I changed since main?"

use std::path::Path;

use git2::{Delta, DiffOptions, Repository};

use crate::error::ServiceError;
use crate::proto::ChangedFile;

pub fn open(repo_path: &Path) -> Result<Repository, ServiceError> {
    if !repo_path.exists() {
        return Err(ServiceError::NotFound(format!(
            "repo path does not exist: {}",
            repo_path.display()
        )));
    }
    Repository::discover(repo_path).map_err(Into::into)
}

pub fn changed_files(repo: &Repository, base_ref: &str) -> Result<Vec<ChangedFile>, ServiceError> {
    let base_commit = resolve_commit(repo, base_ref)?;
    let base_tree = base_commit.tree()?;

    let mut opts = DiffOptions::new();
    opts.include_untracked(true).recurse_untracked_dirs(true);

    let diff = repo.diff_tree_to_workdir_with_index(Some(&base_tree), Some(&mut opts))?;

    let mut out = Vec::new();
    diff.foreach(
        &mut |delta, _| {
            if let Some(path) = file_path_from_delta(&delta) {
                out.push(ChangedFile {
                    path,
                    status: status_label(delta.status()).to_owned(),
                });
            }
            true
        },
        None,
        None,
        None,
    )?;
    Ok(out)
}

pub fn file_diff(repo: &Repository, base_ref: &str, path: &str) -> Result<String, ServiceError> {
    let base_commit = resolve_commit(repo, base_ref)?;
    let base_tree = base_commit.tree()?;

    let mut opts = DiffOptions::new();
    opts.pathspec(path).include_untracked(true).recurse_untracked_dirs(true);

    let diff = repo.diff_tree_to_workdir_with_index(Some(&base_tree), Some(&mut opts))?;

    let mut out = String::new();
    diff.print(git2::DiffFormat::Patch, |_, _, line| {
        match line.origin() {
            '+' | '-' | ' ' => out.push(line.origin()),
            _ => {}
        }
        out.push_str(std::str::from_utf8(line.content()).unwrap_or("<binary>"));
        true
    })?;
    Ok(out)
}

fn resolve_commit<'r>(repo: &'r Repository, base_ref: &str) -> Result<git2::Commit<'r>, ServiceError> {
    let obj = repo.revparse_single(base_ref).map_err(|_| {
        ServiceError::InvalidArgument(format!("could not resolve base_ref '{base_ref}'"))
    })?;
    obj.peel_to_commit().map_err(Into::into)
}

fn file_path_from_delta(delta: &git2::DiffDelta<'_>) -> Option<String> {
    delta
        .new_file()
        .path()
        .or_else(|| delta.old_file().path())
        .map(|p| p.to_string_lossy().into_owned())
}

fn status_label(delta: Delta) -> &'static str {
    match delta {
        Delta::Added => "added",
        Delta::Deleted => "deleted",
        Delta::Modified => "modified",
        Delta::Renamed => "renamed",
        Delta::Copied => "copied",
        Delta::Typechange => "typechange",
        Delta::Untracked => "untracked",
        _ => "unknown",
    }
}
