use crate::error::{Result, invalid};
use serde::{Deserialize, Serialize};
use url::Url;

/// Absolute HTTP(S) repository address supplied for a forge-backed context.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ForgeRepositoryUrl(Url);

impl ForgeRepositoryUrl {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Include scheme and port when scoping remote account identities.
    pub fn origin(&self) -> String {
        self.0.origin().ascii_serialization()
    }

    pub fn parse(value: &str) -> Result<Self> {
        let mut url =
            Url::parse(value).map_err(|_| invalid("Repository URL must be absolute HTTP(S)"))?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url
                .path_segments()
                .is_none_or(|parts| parts.filter(|part| !part.is_empty()).count() < 2)
        {
            return Err(invalid(
                "Repository URL requires a forge host and namespace/project path, without credentials, query or fragment",
            ));
        }
        let path = url.path().trim_end_matches('/').to_owned();
        url.set_path(&path);
        Ok(Self(url))
    }
}

impl TryFrom<String> for ForgeRepositoryUrl {
    type Error = crate::error::Error;

    fn try_from(value: String) -> Result<Self> {
        Self::parse(&value)
    }
}

impl From<ForgeRepositoryUrl> for String {
    fn from(value: ForgeRepositoryUrl) -> Self {
        value.0.to_string()
    }
}

// tests -----------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_validate_and_normalize_forge_repository_urls() {
        assert_eq!(
            ForgeRepositoryUrl::parse("https://GITHUB.com/acme/repo/")
                .unwrap()
                .as_str(),
            "https://github.com/acme/repo"
        );
        assert!(ForgeRepositoryUrl::parse("http://forge.local:8080/team/subgroup/repo").is_ok());
        for value in [
            "local:/repo",
            "/repo",
            "github.com/acme/repo",
            "file:///repo",
            "https://forge.local",
            "https://forge.local/repo",
            "https://user:pass@forge.local/acme/repo",
            "https://forge.local/acme/repo?q=1",
            "https://forge.local/acme/repo#fragment",
        ] {
            assert!(ForgeRepositoryUrl::parse(value).is_err(), "{value}");
            assert!(
                serde_json::from_str::<ForgeRepositoryUrl>(&serde_json::to_string(value).unwrap())
                    .is_err()
            );
        }
    }
}
