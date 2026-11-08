//! Stateless Git path resolution and `git worktree` CLI helpers.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use crate::error::{Result, RsUtilsError};

/// Resolved canonical paths for a Git repository checkout or linked worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedGitPaths {
    /// Canonical root directory of the primary repository checkout owning the shared `.git` directory.
    pub main_root: PathBuf,
    /// Canonical root directory of the inspected checkout or linked worktree.
    pub worktree_root: PathBuf,
    /// Canonical `.git` directory for the inspected worktree (`.git` or `.git/worktrees/<name>`).
    pub worktree_git_dir: PathBuf,
    /// Canonical shared `.git` directory for the repository (`commondir`).
    pub common_git_dir: PathBuf,
}

/// Entry returned by `git worktree list --porcelain`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitWorktreeEntry {
    /// Canonical checkout directory path for the worktree.
    pub path: PathBuf,
    /// Checked-out `HEAD` commit SHA, if available.
    pub head_sha: Option<String>,
    /// Short branch name (without `refs/heads/`), or `None` when detached.
    pub branch: Option<String>,
    /// Whether this entry is the repository's primary checkout rather than a linked worktree.
    pub is_main: bool,
}

/// Resolves any working tree or `.git` path into its canonical `main_root`, `worktree_root`, `worktree_git_dir`, and `common_git_dir`.
pub fn resolve_git_paths(git_dir_or_worktree: &Path) -> Result<ResolvedGitPaths> {
    let raw = git_dir_or_worktree.to_string_lossy();
    if raw.trim().is_empty() {
        return Err(RsUtilsError::Invalid(
            "git_dir must not be empty".to_owned(),
        ));
    }
    let canonical = std::fs::canonicalize(git_dir_or_worktree).map_err(|err| {
        RsUtilsError::Invalid(format!(
            "failed to resolve git_dir '{}': {err}",
            git_dir_or_worktree.display()
        ))
    })?;

    let (worktree_git_dir, worktree_root) =
        if canonical.file_name().is_some_and(|name| name == ".git") {
            let worktree = canonical
                .parent()
                .ok_or_else(|| RsUtilsError::Invalid("git_dir has no parent worktree".to_owned()))?
                .to_path_buf();
            let resolved_git = if canonical.is_file() {
                resolve_gitdir_file(&canonical)?
            } else {
                canonical
            };
            (resolved_git, worktree)
        } else if canonical.join(".git").exists() {
            let dot_git = canonical.join(".git");
            let resolved_git = if dot_git.is_file() {
                resolve_gitdir_file(&dot_git)?
            } else {
                dot_git
            };
            (resolved_git, canonical)
        } else if canonical.join("gitdir").is_file() && canonical.join("commondir").is_file() {
            let wt_git_file = std::fs::read_to_string(canonical.join("gitdir")).map_err(|err| {
                RsUtilsError::Invalid(format!(
                    "failed to read worktree gitdir '{}': {err}",
                    canonical.display()
                ))
            })?;
            let wt_dot_git = Path::new(wt_git_file.trim());
            let wt_dir = wt_dot_git
                .parent()
                .map(|p| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf()))
                .unwrap_or_else(|| canonical.clone());
            (canonical, wt_dir)
        } else {
            (canonical.clone(), canonical)
        };

    let common_git_dir = resolve_common_git_dir(&worktree_git_dir)?;
    let main_root = if common_git_dir
        .file_name()
        .is_some_and(|name| name == ".git")
    {
        common_git_dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| worktree_root.clone())
    } else {
        worktree_root.clone()
    };

    Ok(ResolvedGitPaths {
        main_root,
        worktree_root,
        worktree_git_dir,
        common_git_dir,
    })
}

/// Reads the short checked-out branch name for `checkout_dir`, returning `None` for non-Git folders or detached `HEAD`.
pub fn get_current_branch(checkout_dir: &Path) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(checkout_dir)
        .args([" symbolic-ref".trim(), "--quiet", "--short", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let branch = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!branch.is_empty()).then_some(branch)
}

/// Lists all Git worktrees (primary checkout first, followed by linked worktrees) for `repo_root`.
pub fn list_worktrees(repo_root: &Path) -> Result<Vec<GitWorktreeEntry>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["worktree", "list", "--porcelain"])
        .output()
        .map_err(|err| RsUtilsError::Git(format!("failed to invoke git worktree list: {err}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(RsUtilsError::Git(format!(
            "git worktree list failed: {}",
            stderr.trim()
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut entries = Vec::new();
    let mut current_path: Option<PathBuf> = None;
    let mut current_head: Option<String> = None;
    let mut current_branch: Option<String> = None;

    let flush = |entries: &mut Vec<GitWorktreeEntry>,
                 path: Option<PathBuf>,
                 head: Option<String>,
                 branch: Option<String>| {
        if let Some(raw_path) = path {
            let canonical = std::fs::canonicalize(&raw_path).unwrap_or(raw_path);
            let is_main = entries.is_empty();
            entries.push(GitWorktreeEntry {
                path: canonical,
                head_sha: head,
                branch,
                is_main,
            });
        }
    };

    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            flush(
                &mut entries,
                current_path.take(),
                current_head.take(),
                current_branch.take(),
            );
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("worktree ") {
            flush(
                &mut entries,
                current_path.take(),
                current_head.take(),
                current_branch.take(),
            );
            current_path = Some(PathBuf::from(rest.trim()));
        } else if let Some(rest) = trimmed.strip_prefix("HEAD ") {
            current_head = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("branch ") {
            let short = rest
                .trim()
                .strip_prefix("refs/heads/")
                .unwrap_or(rest.trim())
                .to_owned();
            current_branch = Some(short);
        }
    }
    flush(&mut entries, current_path, current_head, current_branch);

    Ok(entries)
}

/// Creates a linked Git worktree at `target_dir` for `branch` (reusing `branch` if it exists locally or creating `-b <branch>` from `base_ref`).
pub fn add_worktree(
    repo_root: &Path,
    target_dir: &Path,
    branch: &str,
    base_ref: Option<&str>,
) -> Result<GitWorktreeEntry> {
    let branch = branch.trim();
    if branch.is_empty()
        || branch.starts_with('-')
        || branch.contains("..")
        || branch.contains(char::is_whitespace)
    {
        return Err(RsUtilsError::Invalid(
            "Enter a valid Git branch name.".to_owned(),
        ));
    }
    let check_format = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["check-ref-format", "--branch", branch])
        .output()
        .map_err(|err| {
            RsUtilsError::Git(format!("failed to invoke git check-ref-format: {err}"))
        })?;
    if !check_format.status.success() {
        return Err(RsUtilsError::Invalid(format!(
            "'{branch}' is not a valid Git branch name."
        )));
    }
    if target_dir.exists() {
        return Err(RsUtilsError::Invalid(format!(
            "Target worktree path '{}' already exists.",
            target_dir.display()
        )));
    }
    if let Some(parent) = target_dir.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let branch_exists = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args([
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ])
        .status()
        .is_ok_and(|status| status.success());

    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(repo_root).args(["worktree", "add"]);
    if branch_exists {
        cmd.arg(target_dir).arg(branch);
    } else {
        cmd.arg("-b").arg(branch).arg(target_dir);
        if let Some(base) = base_ref.map(str::trim).filter(|s| !s.is_empty()) {
            cmd.arg(base);
        }
    }

    let output = cmd
        .output()
        .map_err(|err| RsUtilsError::Git(format!("failed to invoke git worktree add: {err}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        if stderr.contains("already checked out") || stderr.contains("already used by worktree") {
            return Err(RsUtilsError::BranchAlreadyCheckedOut(stderr));
        }
        return Err(RsUtilsError::Git(format!(
            "git worktree add failed: {stderr}"
        )));
    }

    let canonical_target =
        std::fs::canonicalize(target_dir).unwrap_or_else(|_| target_dir.to_path_buf());
    Ok(GitWorktreeEntry {
        path: canonical_target,
        head_sha: None,
        branch: Some(branch.to_owned()),
        is_main: false,
    })
}

/// Removes a linked Git worktree at `worktree_dir`, classifying dirty-checkout errors when `force` is `false`.
pub fn remove_worktree(repo_root: &Path, worktree_dir: &Path, force: bool) -> Result<()> {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(repo_root).args(["worktree", "remove"]);
    if force {
        cmd.arg("--force");
    }
    cmd.arg(worktree_dir);

    let output = cmd
        .output()
        .map_err(|err| RsUtilsError::Git(format!("failed to invoke git worktree remove: {err}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        if !force
            && (stderr.contains("modified or untracked")
                || stderr.contains("contains modified")
                || stderr.contains("use --force"))
        {
            return Err(RsUtilsError::DirtyWorktree(stderr));
        }
        return Err(RsUtilsError::Git(format!(
            "git worktree remove failed: {stderr}"
        )));
    }
    if worktree_dir.exists() {
        let _ = std::fs::remove_dir_all(worktree_dir);
    }
    Ok(())
}

fn resolve_common_git_dir(worktree_git_dir: &Path) -> Result<PathBuf> {
    let commondir_file = worktree_git_dir.join("commondir");
    if commondir_file.is_file() {
        let raw = std::fs::read_to_string(&commondir_file).map_err(|err| {
            RsUtilsError::Invalid(format!(
                "failed to read commondir file '{}': {err}",
                commondir_file.display()
            ))
        })?;
        let rel = raw.trim();
        let target = if Path::new(rel).is_absolute() {
            PathBuf::from(rel)
        } else {
            worktree_git_dir.join(rel)
        };
        return std::fs::canonicalize(&target).map_err(|err| {
            RsUtilsError::Invalid(format!(
                "failed to canonicalize commondir target '{}': {err}",
                target.display()
            ))
        });
    }
    Ok(worktree_git_dir.to_path_buf())
}

fn resolve_gitdir_file(dot_git_file: &Path) -> Result<PathBuf> {
    let contents = std::fs::read_to_string(dot_git_file).map_err(|err| {
        RsUtilsError::Invalid(format!(
            "failed to read .git file '{}': {err}",
            dot_git_file.display()
        ))
    })?;
    let raw_target = contents
        .trim()
        .strip_prefix("gitdir:")
        .ok_or_else(|| {
            RsUtilsError::Invalid(format!("malformed .git file '{}'", dot_git_file.display()))
        })?
        .trim();
    let target_path = Path::new(raw_target);
    let resolved = if target_path.is_absolute() {
        target_path.to_path_buf()
    } else {
        dot_git_file
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(target_path)
    };
    std::fs::canonicalize(&resolved).map_err(|err| {
        RsUtilsError::Invalid(format!(
            "failed to canonicalize gitdir target '{}': {err}",
            resolved.display()
        ))
    })
}
