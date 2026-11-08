//! General filesystem path, LGTM home, remote URL, and agent descriptor utilities.

use std::path::PathBuf;

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

/// Normalize a Git remote URL (SSH or HTTPS) into a canonical `https://<host>/<owner>/<repo>` URL.
pub fn normalize_repository_url(raw_remote: &str) -> Result<String> {
    lgtm_rsutils::normalize_repository_url(raw_remote)
        .map_err(|err| Error::Invalid(err.to_string()))
}

/// Extract `(owner, repo)` from a normalized `repository_url`.
pub fn parse_github_owner_repo(repository_url: &str) -> Result<(String, String)> {
    lgtm_rsutils::parse_github_owner_repo(repository_url)
        .map_err(|err| Error::Invalid(err.to_string()))
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
