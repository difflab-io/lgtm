//! Remote Git repository URL normalization and `<owner>/<repo>` extraction.

use std::io::{Error, ErrorKind, Result};

use url::Url;

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
