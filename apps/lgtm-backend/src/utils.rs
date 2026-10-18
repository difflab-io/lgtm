//! Shared formatting helpers and forge import/export contracts.
use crate::models::Anchor;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub fn within(path: &str, root: &str) -> bool {
    Path::new(path).starts_with(Path::new(root))
}

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
    pub resolved: bool,
    pub comments: Vec<RemoteComment>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Complete or partial fetch result used to refresh a forge context.
pub struct ForgeSnapshot {
    pub complete: bool,
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

/// A UTF-8 text file at the selected content revision.
#[derive(Clone, Debug)]
pub struct WorkspaceFile {
    pub path: String,
    pub content: String,
    pub content_hash: String,
    pub status: String,
}

/// Picker entry containing immutable reviewed revision IDs.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PullRequest {
    pub number: i64,
    pub title: String,
    pub head: String,
    pub base: String,
    pub repository_url: String,
}
