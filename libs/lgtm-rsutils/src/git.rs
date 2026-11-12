//! Domain-agnostic `gitoxide` (`gix`) inspection, `git worktree` helpers, and remote Git repository URL normalization.

use std::{
    io::{Error, ErrorKind, Result},
    path::{Path, PathBuf},
    process::Command,
};

use gix::bstr::ByteSlice;
use url::Url;

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

/// Metadata for a primary or linked Git worktree discovered via `gix`.
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
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "git_dir must not be empty",
        ));
    }
    let canonical = std::fs::canonicalize(git_dir_or_worktree).map_err(|err| {
        Error::new(
            ErrorKind::InvalidInput,
            format!(
                "failed to resolve git_dir '{}': {err}",
                git_dir_or_worktree.display()
            ),
        )
    })?;

    if let Ok(repo) = gix::open(&canonical) {
        let worktree_git_dir =
            std::fs::canonicalize(repo.git_dir()).unwrap_or_else(|_| repo.git_dir().to_path_buf());
        let common_git_dir = std::fs::canonicalize(repo.common_dir())
            .unwrap_or_else(|_| repo.common_dir().to_path_buf());
        let worktree_root = if let Some(workdir) = repo.work_dir() {
            std::fs::canonicalize(workdir).unwrap_or_else(|_| workdir.to_path_buf())
        } else if worktree_git_dir.join("gitdir").is_file() {
            resolve_worktree_root_from_gitdir(&worktree_git_dir)
                .unwrap_or_else(|| worktree_git_dir.clone())
        } else if worktree_git_dir
            .file_name()
            .is_some_and(|name| name == ".git")
        {
            worktree_git_dir
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| worktree_git_dir.clone())
        } else {
            worktree_git_dir.clone()
        };
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
        return Ok(ResolvedGitPaths {
            main_root,
            worktree_root,
            worktree_git_dir,
            common_git_dir,
        });
    }

    let (worktree_git_dir, worktree_root) = if canonical
        .file_name()
        .is_some_and(|name| name == ".git")
    {
        let worktree = canonical
            .parent()
            .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "git_dir has no parent worktree"))?
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
        let wt_dir =
            resolve_worktree_root_from_gitdir(&canonical).unwrap_or_else(|| canonical.clone());
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

/// Reads the short checked-out branch name for `checkout_dir` via `gix`, returning `None` for non-Git folders or detached `HEAD`.
pub fn get_current_branch(checkout_dir: &Path) -> Option<String> {
    let repo = gix::open(checkout_dir).ok()?;
    let head_name = repo.head_name().ok()??;
    let branch = head_name.shorten().to_str().ok()?.trim().to_owned();
    (!branch.is_empty()).then_some(branch)
}

/// Lists all Git worktrees (primary checkout first, followed by linked worktrees) for `repo_root` via `gix`.
pub fn list_worktrees(repo_root: &Path) -> Result<Vec<GitWorktreeEntry>> {
    let resolved = resolve_git_paths(repo_root)?;
    let main_repo = gix::open(&resolved.main_root).map_err(|err| {
        Error::other(format!(
            "failed to open repository '{}': {err}",
            resolved.main_root.display()
        ))
    })?;

    let mut entries = Vec::new();
    entries.push(inspect_worktree_entry(&resolved.main_root, true));

    let proxies = main_repo
        .worktrees()
        .map_err(|err| Error::other(format!("failed to enumerate git worktrees: {err}")))?;
    for proxy in proxies {
        if let Ok(base) = proxy.base() {
            let canonical = std::fs::canonicalize(&base).unwrap_or(base);
            entries.push(inspect_worktree_entry(&canonical, false));
        }
    }

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
        || branch.starts_with('/')
        || branch.ends_with('/')
        || branch.contains("..")
        || branch.contains("//")
        || branch.contains(char::is_whitespace)
        || gix::validate::reference::name_partial(branch.as_bytes().as_bstr()).is_err()
    {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            format!("'{branch}' is not a valid Git branch name."),
        ));
    }
    if target_dir.exists() {
        return Err(Error::new(
            ErrorKind::AlreadyExists,
            format!(
                "Target worktree path '{}' already exists.",
                target_dir.display()
            ),
        ));
    }
    if let Some(parent) = target_dir.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let branch_exists = gix::open(repo_root)
        .ok()
        .and_then(|repo| {
            repo.find_reference(&format!("refs/heads/{branch}"))
                .ok()
                .map(|_| ())
        })
        .is_some();

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
        .map_err(|err| Error::other(format!("failed to invoke git worktree add: {err}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        if stderr.contains("already checked out") || stderr.contains("already used by worktree") {
            return Err(Error::new(ErrorKind::AlreadyExists, stderr));
        }
        return Err(Error::other(format!("git worktree add failed: {stderr}")));
    }

    let canonical_target =
        std::fs::canonicalize(target_dir).unwrap_or_else(|_| target_dir.to_path_buf());
    Ok(inspect_worktree_entry(&canonical_target, false))
}

/// Removes a linked Git worktree at `worktree_dir`, returning `ErrorKind::WouldBlock` when `force` is `false` and the checkout has uncommitted changes.
pub fn remove_worktree(repo_root: &Path, worktree_dir: &Path, force: bool) -> Result<()> {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(repo_root).args(["worktree", "remove"]);
    if force {
        cmd.arg("--force");
    }
    cmd.arg(worktree_dir);

    let output = cmd
        .output()
        .map_err(|err| Error::other(format!("failed to invoke git worktree remove: {err}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        if !force
            && (stderr.contains("modified or untracked")
                || stderr.contains("contains modified")
                || stderr.contains("use --force"))
        {
            return Err(Error::new(ErrorKind::WouldBlock, stderr));
        }
        return Err(Error::other(format!(
            "git worktree remove failed: {stderr}"
        )));
    }
    if worktree_dir.exists() {
        let _ = std::fs::remove_dir_all(worktree_dir);
    }
    Ok(())
}

/// Normalizes a Git remote URL (SSH or HTTPS) into a canonical `https://<host>/<owner>/<repo>` URL.
pub fn normalize_repository_url(raw_remote: &str) -> Result<String> {
    let trimmed = raw_remote.trim();
    if trimmed.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "repository_url must not be empty",
        ));
    }

    if let Some(rest) = trimmed.strip_prefix("git@") {
        let (host, path) = rest.split_once(':').ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidInput,
                format!("invalid SSH remote address '{trimmed}'"),
            )
        })?;
        let clean_path = path.trim_start_matches('/').trim_end_matches(".git");
        if clean_path.split('/').count() < 2 {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                format!("repository_url '{trimmed}' must include owner and repository name"),
            ));
        }
        return Ok(format!("https://{host}/{clean_path}"));
    }

    let parsed = Url::parse(trimmed).map_err(|err| {
        Error::new(
            ErrorKind::InvalidInput,
            format!("invalid repository_url '{trimmed}': {err}"),
        )
    })?;
    let scheme = match parsed.scheme() {
        "http" | "https" => parsed.scheme(),
        "ssh" => "https",
        other => {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                format!("unsupported repository_url scheme '{other}'"),
            ));
        }
    };
    let host = parsed.host_str().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidInput,
            format!("repository_url '{trimmed}' is missing a host"),
        )
    })?;
    let clean_path = parsed
        .path()
        .trim_start_matches('/')
        .trim_end_matches('/')
        .trim_end_matches(".git");
    if clean_path
        .split('/')
        .filter(|part| !part.is_empty())
        .count()
        < 2
    {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            format!("repository_url '{trimmed}' must include owner and repository name"),
        ));
    }
    Ok(format!("{scheme}://{host}/{clean_path}"))
}

/// Extracts `(owner, repo)` from a normalized or raw `repository_url`.
pub fn parse_github_owner_repo(repository_url: &str) -> Result<(String, String)> {
    let normalized = normalize_repository_url(repository_url)?;
    let parsed = Url::parse(&normalized).map_err(|err| {
        Error::new(
            ErrorKind::InvalidInput,
            format!("invalid repository_url '{normalized}': {err}"),
        )
    })?;
    let mut segments = parsed
        .path_segments()
        .ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidInput,
                format!("invalid repository_url '{normalized}'"),
            )
        })?
        .filter(|segment| !segment.is_empty());
    let owner = segments
        .next()
        .ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidInput,
                format!("missing owner in '{normalized}'"),
            )
        })?
        .to_owned();
    let repo = segments
        .next()
        .ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidInput,
                format!("missing repository name in '{normalized}'"),
            )
        })?
        .to_owned();
    Ok((owner, repo))
}

fn inspect_worktree_entry(path: &Path, is_main: bool) -> GitWorktreeEntry {
    let (head_sha, branch) = match gix::open(path) {
        Ok(repo) => {
            let head_sha = repo.head_id().ok().map(|id| id.to_hex().to_string());
            let branch = repo
                .head_name()
                .ok()
                .flatten()
                .and_then(|name| name.shorten().to_str().ok().map(str::to_owned));
            (head_sha, branch)
        }
        Err(_) => (None, None),
    };
    GitWorktreeEntry {
        path: path.to_path_buf(),
        head_sha,
        branch,
        is_main,
    }
}

fn resolve_worktree_root_from_gitdir(worktree_git_dir: &Path) -> Option<PathBuf> {
    let wt_git_file = std::fs::read_to_string(worktree_git_dir.join("gitdir")).ok()?;
    let wt_dot_git = Path::new(wt_git_file.trim());
    wt_dot_git
        .parent()
        .map(|p| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf()))
}

fn resolve_common_git_dir(worktree_git_dir: &Path) -> Result<PathBuf> {
    let commondir_file = worktree_git_dir.join("commondir");
    if commondir_file.is_file() {
        let raw = std::fs::read_to_string(&commondir_file).map_err(|err| {
            Error::new(
                ErrorKind::InvalidInput,
                format!(
                    "failed to read commondir file '{}': {err}",
                    commondir_file.display()
                ),
            )
        })?;
        let rel = raw.trim();
        let target = if Path::new(rel).is_absolute() {
            PathBuf::from(rel)
        } else {
            worktree_git_dir.join(rel)
        };
        return std::fs::canonicalize(&target).map_err(|err| {
            Error::new(
                ErrorKind::InvalidInput,
                format!(
                    "failed to canonicalize commondir target '{}': {err}",
                    target.display()
                ),
            )
        });
    }
    Ok(worktree_git_dir.to_path_buf())
}

fn resolve_gitdir_file(dot_git_file: &Path) -> Result<PathBuf> {
    let contents = std::fs::read_to_string(dot_git_file).map_err(|err| {
        Error::new(
            ErrorKind::InvalidInput,
            format!(
                "failed to read .git file '{}': {err}",
                dot_git_file.display()
            ),
        )
    })?;
    let raw_target = contents
        .trim()
        .strip_prefix("gitdir:")
        .ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidInput,
                format!("malformed .git file '{}'", dot_git_file.display()),
            )
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
        Error::new(
            ErrorKind::InvalidInput,
            format!(
                "failed to canonicalize gitdir target '{}': {err}",
                resolved.display()
            ),
        )
    })
}

// tests -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // tests -------------------------------------------------------------------

    mod given_remote_git_urls {
        use super::*;

        mod when_normalizing_and_extracting_owner_repo {
            use super::*;

            #[test]
            fn should_normalize_ssh_and_https_repository_urls() {
                // Arrange
                let ssh_url = "git@github.com:difflab-io/lgtm.git";
                let https_url = "https://github.com/difflab-io/lgtm.git/";

                // Act
                let normalized_ssh = normalize_repository_url(ssh_url).unwrap();
                let normalized_https = normalize_repository_url(https_url).unwrap();
                let owner_repo = parse_github_owner_repo(ssh_url).unwrap();

                // Assert
                assert_eq!(normalized_ssh, "https://github.com/difflab-io/lgtm");
                assert_eq!(normalized_https, "https://github.com/difflab-io/lgtm");
                assert_eq!(owner_repo, ("difflab-io".to_owned(), "lgtm".to_owned()));
            }
        }
    }
}
