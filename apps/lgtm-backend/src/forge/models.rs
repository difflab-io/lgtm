//! Provider data exchanged with review services.
use crate::{
    error::{Result, invalid},
    models::{Anchor, ContextKey, ForgeObservation, ForgeRepositoryUrl},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Forge account details supplied before matching a local actor.
pub struct RemoteAuthor {
    pub name: String,
    pub email: String,
    pub account_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Fetched forge comment awaiting reconciliation with a local comment.
pub struct RemoteComment {
    /// Private pending-review comment visible only to the posting account.
    pub pending: bool,
    pub id: String,
    pub kind: String,
    pub review_id: Option<String>,
    pub body: String,
    pub updated_at: String,
    pub author: RemoteAuthor,
    pub reply_to: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Fetched thread and comments awaiting reconciliation.
pub struct RemoteThread {
    pub id: String,
    pub anchor: Anchor,
    /// Provider current location, separate from the immutable original anchor.
    #[serde(default)]
    pub observation: Option<ForgeObservation>,
    pub resolved: bool,
    /// A provider-confirmed missing source file; unknown placement alone is not deletion.
    #[serde(default)]
    pub orphan_reason: Option<crate::models::OrphanReason>,
    pub comments: Vec<RemoteComment>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Complete or partial fetch result used to refresh a forge context.
pub struct ForgeSnapshot {
    pub complete: bool,
    #[serde(default)]
    pub state: Option<crate::database::types::ForgeState>,
    pub head: String,
    pub base: Option<String>,
    pub threads: Vec<RemoteThread>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Provider confirmation identifying a successfully posted comment.
pub struct PostReceipt {
    pub comment_id: String,
    pub kind: String,
    pub thread_id: Option<String>,
    pub review_id: Option<String>,
    pub posting_account_id: String,
}

/// Picker entry containing immutable reviewed revision IDs.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PullRequest {
    pub number: i64,
    pub state: crate::database::types::ForgeState,
    pub title: String,
    pub head: String,
    pub base: String,
    pub repository_url: String,
    /// Provider account that opened this PR; absent when the provider does not supply it.
    #[serde(default)]
    pub author_account_id: Option<String>,
}

/// Validated github.com repository scope. IDs remain scoped opaque strings.
#[derive(Clone, Debug)]
pub struct Repository {
    pub owner: String,
    pub name: String,
    pub repository_url: String,
}

impl Repository {
    pub fn route(&self) -> String {
        format!("/repos/{}/{}", self.owner, self.name)
    }

    pub fn from_repository_url(repository_url: &str) -> Result<Self> {
        let repository_url = ForgeRepositoryUrl::parse(repository_url)?;
        let url = url::Url::parse(repository_url.as_str())
            .map_err(|_| invalid("Invalid repository URL"))?;
        if url.scheme() != "https" || url.host_str() != Some("github.com") || url.port().is_some() {
            return Err(invalid(
                "Only https://github.com repositories are supported",
            ));
        }
        let parts: Vec<_> = url.path().trim_matches('/').split('/').collect();
        if parts.len() != 2
            || parts.iter().any(|v| {
                v.is_empty()
                    || !v
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
            })
        {
            return Err(invalid("GitHub repository URL requires owner/repository"));
        }
        let name = parts[1].trim_end_matches(".git");
        if name.is_empty() {
            return Err(invalid("GitHub repository name is empty"));
        }
        Ok(Self {
            owner: parts[0].into(),
            name: name.into(),
            repository_url: format!("https://github.com/{}/{}", parts[0], name),
        })
    }

    pub fn from_context_key(key: &ContextKey) -> Result<(Self, i64)> {
        match key {
            ContextKey::ForgeBacked {
                repository_url,
                number,
            } => Ok((Self::from_repository_url(repository_url.as_str())?, *number)),
            _ => Err(invalid("Remote operations require a forge-backed context")),
        }
    }
}
