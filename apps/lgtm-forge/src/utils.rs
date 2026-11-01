//! General filesystem path, repository slug, and remote URL utilities.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use url::Url;

use crate::error::{Error, Result};

/// Resolve the default root directory (`~/.lgtm/snapshots` or `$LGTM_HOME/snapshots`) for local snapshot repositories.
pub fn default_snapshots_root() -> PathBuf {
    default_lgtm_home().join("snapshots")
}

/// Resolve the default database URL (`LGTM_DATABASE_URL`, `DATABASE_URL`, or `sqlite://$LGTM_HOME/lgtm.db?mode=rwc`).
pub fn default_database_url() -> Result<String> {
    for var in ["LGTM_DATABASE_URL", "DATABASE_URL"] {
        if let Ok(url) = std::env::var(var)
            && !url.trim().is_empty()
        {
            return Ok(url.trim().to_owned());
        }
    }
    let lgtm_home = default_lgtm_home();
    std::fs::create_dir_all(&lgtm_home).map_err(|err| {
        Error::Invalid(format!(
            "failed to create LGTM home directory '{}': {err}",
            lgtm_home.display()
        ))
    })?;
    let db_path = lgtm_home.join("lgtm.db");
    Ok(format!("sqlite://{}?mode=rwc", db_path.display()))
}

fn default_lgtm_home() -> PathBuf {
    if let Ok(home) = std::env::var("LGTM_HOME")
        && !home.trim().is_empty()
    {
        return PathBuf::from(home.trim());
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".lgtm")
}

/// Normalize a working tree or `.git` path to the repository's canonical shared `common_dir` (for `$repoSlug` and DB scoping) and the active worktree's root directory.
pub fn resolve_git_paths(git_dir_or_worktree: &str) -> Result<(PathBuf, PathBuf, String)> {
    let trimmed = git_dir_or_worktree.trim();
    if trimmed.is_empty() {
        return Err(Error::Invalid("git_dir must not be empty".to_owned()));
    }
    let input_path = Path::new(trimmed);
    let canonical = std::fs::canonicalize(input_path).map_err(|err| {
        Error::Invalid(format!(
            "failed to resolve git_dir '{}': {err}",
            input_path.display()
        ))
    })?;

    let (worktree_git_dir, worktree_dir) =
        if canonical.file_name().is_some_and(|name| name == ".git") {
            let worktree = canonical
                .parent()
                .ok_or_else(|| Error::Invalid("git_dir has no parent worktree".to_owned()))?
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
                Error::Invalid(format!(
                    "failed to read worktree gitdir '{}': {err}",
                    canonical.display()
                ))
            })?;
            let wt_dot_git = Path::new(wt_git_file.trim());
            let wt_dir = wt_dot_git
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| canonical.clone());
            (canonical, wt_dir)
        } else {
            (canonical.clone(), canonical)
        };

    let common_git_dir = resolve_common_git_dir(&worktree_git_dir)?;
    let normalized_git_dir = common_git_dir.to_string_lossy().into_owned();
    Ok((worktree_git_dir, worktree_dir, normalized_git_dir))
}

/// Compute a deterministic, filesystem-safe `$repoSlug` for a repository's shared `common_dir`.
pub fn compute_repo_slug(git_dir: &str) -> Result<String> {
    let (_, worktree_dir, normalized_git_dir) = resolve_git_paths(git_dir)?;
    let common_path = Path::new(&normalized_git_dir);
    let repo_root = if common_path.file_name().is_some_and(|name| name == ".git") {
        common_path.parent().unwrap_or(&worktree_dir)
    } else {
        common_path
    };
    let repo_name = repo_root
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("repo")
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();

    let mut hasher = Sha256::new();
    hasher.update(normalized_git_dir.as_bytes());
    let digest = hasher.finalize();
    let short_hash = digest[..6]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();

    Ok(format!("{repo_name}-{short_hash}"))
}

/// Normalize a Git remote URL (SSH or HTTPS) into a canonical `https://<host>/<owner>/<repo>` URL.
pub fn normalize_repository_url(raw_remote: &str) -> Result<String> {
    let trimmed = raw_remote.trim();
    if trimmed.is_empty() {
        return Err(Error::Invalid(
            "repository_url must not be empty".to_owned(),
        ));
    }

    if let Some(rest) = trimmed.strip_prefix("git@") {
        let (host, path) = rest
            .split_once(':')
            .ok_or_else(|| Error::Invalid(format!("invalid SSH remote address '{trimmed}'")))?;
        let clean_path = path.trim_start_matches('/').trim_end_matches(".git");
        if clean_path.split('/').count() < 2 {
            return Err(Error::Invalid(format!(
                "repository_url '{trimmed}' must include owner and repository name"
            )));
        }
        return Ok(format!("https://{host}/{clean_path}"));
    }

    let parsed = Url::parse(trimmed)
        .map_err(|err| Error::Invalid(format!("invalid repository_url '{trimmed}': {err}")))?;
    let scheme = match parsed.scheme() {
        "http" | "https" => parsed.scheme(),
        "ssh" => "https",
        other => {
            return Err(Error::Invalid(format!(
                "unsupported repository_url scheme '{other}'"
            )));
        }
    };
    let host = parsed
        .host_str()
        .ok_or_else(|| Error::Invalid(format!("repository_url '{trimmed}' is missing a host")))?;
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
        return Err(Error::Invalid(format!(
            "repository_url '{trimmed}' must include owner and repository name"
        )));
    }
    Ok(format!("{scheme}://{host}/{clean_path}"))
}

/// Extract `(owner, repo)` from a normalized `repository_url`.
pub fn parse_github_owner_repo(repository_url: &str) -> Result<(String, String)> {
    let normalized = normalize_repository_url(repository_url)?;
    let parsed = Url::parse(&normalized)
        .map_err(|err| Error::Invalid(format!("invalid repository_url '{normalized}': {err}")))?;
    let mut segments = parsed
        .path_segments()
        .ok_or_else(|| Error::Invalid(format!("invalid repository_url '{normalized}'")))?
        .filter(|segment| !segment.is_empty());
    let owner = segments
        .next()
        .ok_or_else(|| Error::Invalid(format!("missing owner in '{normalized}'")))?
        .to_owned();
    let repo = segments
        .next()
        .ok_or_else(|| Error::Invalid(format!("missing repository name in '{normalized}'")))?
        .to_owned();
    Ok((owner, repo))
}

/// Parse an `--as` agent descriptor (`provider/model` or `model`) into `(name, provider, model)`.
pub fn parse_agent_descriptor(raw: &str) -> Result<(String, String, String)> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(Error::Invalid(
            "agent identifier must not be empty".to_owned(),
        ));
    }
    if let Some((provider, model)) = trimmed.split_once('/') {
        let provider = provider.trim();
        let model = model.trim();
        if provider.is_empty() || model.is_empty() {
            return Err(Error::Invalid(format!(
                "invalid agent identifier '{trimmed}'; expected '<provider>/<model>' or '<model>'"
            )));
        }
        return Ok((model.to_owned(), provider.to_owned(), model.to_owned()));
    }

    let lower = trimmed.to_ascii_lowercase();
    let provider = if lower.starts_with("claude") || lower.contains("anthropic") {
        "anthropic"
    } else if lower.starts_with("gpt") || lower.starts_with("o1") || lower.starts_with("o3") {
        "openai"
    } else if lower.starts_with("gemini") {
        "google"
    } else {
        "custom"
    };
    Ok((trimmed.to_owned(), provider.to_owned(), trimmed.to_owned()))
}

fn resolve_common_git_dir(worktree_git_dir: &Path) -> Result<PathBuf> {
    let commondir_file = worktree_git_dir.join("commondir");
    if commondir_file.is_file() {
        let raw = std::fs::read_to_string(&commondir_file).map_err(|err| {
            Error::Invalid(format!(
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
            Error::Invalid(format!(
                "failed to canonicalize commondir target '{}': {err}",
                target.display()
            ))
        });
    }
    Ok(worktree_git_dir.to_path_buf())
}

fn resolve_gitdir_file(dot_git_file: &Path) -> Result<PathBuf> {
    let contents = std::fs::read_to_string(dot_git_file).map_err(|err| {
        Error::Invalid(format!(
            "failed to read .git file '{}': {err}",
            dot_git_file.display()
        ))
    })?;
    let raw_target = contents
        .trim()
        .strip_prefix("gitdir:")
        .ok_or_else(|| Error::Invalid(format!("malformed .git file '{}'", dot_git_file.display())))?
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
        Error::Invalid(format!(
            "failed to canonicalize gitdir target '{}': {err}",
            resolved.display()
        ))
    })
}

// tests -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_normalize_ssh_and_https_repository_urls() {
        assert_eq!(
            normalize_repository_url("git@github.com:difflab-io/lgtm.git").unwrap(),
            "https://github.com/difflab-io/lgtm"
        );
        assert_eq!(
            normalize_repository_url("https://github.com/difflab-io/lgtm.git/").unwrap(),
            "https://github.com/difflab-io/lgtm"
        );
        assert_eq!(
            parse_github_owner_repo("git@github.com:difflab-io/lgtm.git").unwrap(),
            ("difflab-io".to_owned(), "lgtm".to_owned())
        );
    }

    #[test]
    fn should_parse_agent_descriptors_with_default_or_explicit_provider() {
        assert_eq!(
            parse_agent_descriptor("claude-sonnet-4-5").unwrap(),
            (
                "claude-sonnet-4-5".to_owned(),
                "anthropic".to_owned(),
                "claude-sonnet-4-5".to_owned()
            )
        );
        assert_eq!(
            parse_agent_descriptor("openai/gpt-5").unwrap(),
            ("gpt-5".to_owned(), "openai".to_owned(), "gpt-5".to_owned())
        );
    }
}
