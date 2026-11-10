//! General filesystem path, LGTM home, repository slug, and agent descriptor utilities.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::error::{Error, Result};

/// Resolves the default LGTM home directory (`$LGTM_HOME` if set and non-empty, otherwise `~/.lgtm`).
pub fn default_lgtm_home() -> PathBuf {
    if let Ok(home) = std::env::var("LGTM_HOME")
        && !home.trim().is_empty()
    {
        return PathBuf::from(home.trim());
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".lgtm")
}

/// Computes a deterministic, filesystem-safe `<repo-name>-<12-hex-chars>` slug for a repository's shared `common_git_dir`.
pub fn compute_repo_slug(git_dir_or_worktree: &Path) -> Result<String> {
    let resolved = lgtm_rsutils::resolve_git_paths(git_dir_or_worktree)?;
    let repo_name = resolved
        .main_root
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
    hasher.update(resolved.common_git_dir.to_string_lossy().as_bytes());
    let digest = hasher.finalize();
    let full_hex = format!("{digest:x}");
    let short_hash = &full_hex[..12];

    Ok(format!("{repo_name}-{short_hash}"))
}

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
    }
    .to_owned();
    Ok((trimmed.to_owned(), provider, trimmed.to_owned()))
}

// tests -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // tests -------------------------------------------------------------------

    mod given_agent_descriptors {
        use super::*;

        mod when_parsing_shorthand_or_qualified_names {
            use super::*;

            #[test]
            fn should_parse_agent_descriptors_with_default_or_explicit_provider() {
                // Arrange
                let shorthand = "claude-sonnet-4-5";
                let qualified = "openai/gpt-5";

                // Act
                let parsed_shorthand = parse_agent_descriptor(shorthand).unwrap();
                let parsed_qualified = parse_agent_descriptor(qualified).unwrap();

                // Assert
                assert_eq!(
                    parsed_shorthand,
                    (
                        "claude-sonnet-4-5".to_owned(),
                        "anthropic".to_owned(),
                        "claude-sonnet-4-5".to_owned()
                    )
                );
                assert_eq!(
                    parsed_qualified,
                    ("gpt-5".to_owned(), "openai".to_owned(), "gpt-5".to_owned())
                );
            }
        }
    }
}
