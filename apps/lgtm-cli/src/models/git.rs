//! View models for `lgtm add`, `lgtm commit`, and `lgtm push` output.

use serde_json::json;

use crate::{
    backend::proto::lgtm::v1::{AddResponse, CommitResponse, PushResponse},
    models::common::{IdentityView, short_sha},
};

/// View model for `lgtm add`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddView {
    pub staged_paths: Vec<String>,
    pub local: bool,
}

impl AddView {
    // Public ------------------------------------------------------------------

    /// Print the staged paths in terminal or `--json` format.
    pub fn print(&self, json: bool) {
        if json {
            println!(
                "{}",
                json!({
                    "staged_paths": self.staged_paths,
                    "local": self.local,
                })
            );
        } else {
            let mode = if self.local {
                "local snapshot"
            } else {
                "working tree"
            };
            println!(
                "Staged {} path(s) ({mode}): {}",
                self.staged_paths.len(),
                self.staged_paths.join(", ")
            );
        }
    }
}

impl From<(AddResponse, bool)> for AddView {
    fn from((response, local): (AddResponse, bool)) -> Self {
        Self {
            staged_paths: response.staged_paths,
            local,
        }
    }
}

/// View model for `lgtm commit`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitView {
    pub commit_sha: String,
    pub branch: String,
    pub author: Option<IdentityView>,
    pub local: bool,
}

impl CommitView {
    // Public ------------------------------------------------------------------

    /// Print the commit summary in terminal or `--json` format.
    pub fn print(&self, json: bool) {
        if json {
            println!(
                "{}",
                json!({
                    "commit_sha": self.commit_sha,
                    "branch": self.branch,
                    "author": self.author.as_ref().map(IdentityView::to_json),
                    "local": self.local,
                })
            );
        } else {
            let mode = if self.local {
                "local snapshot"
            } else {
                "working tree"
            };
            let author_label = self
                .author
                .as_ref()
                .map_or_else(|| "unknown".to_owned(), IdentityView::display_label);
            println!(
                "Committed {} on {} ({mode}) by {author_label}",
                short_sha(&self.commit_sha),
                self.branch
            );
        }
    }
}

impl From<CommitResponse> for CommitView {
    fn from(response: CommitResponse) -> Self {
        Self {
            commit_sha: response.commit_sha,
            branch: response.branch,
            author: response.author.as_option().map(IdentityView::from),
            local: response.local,
        }
    }
}

/// View model for `lgtm push`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PushView {
    pub branch: String,
    pub head_sha: String,
    pub repository_url: String,
    pub local: bool,
}

impl PushView {
    // Public ------------------------------------------------------------------

    /// Print the push outcome in terminal or `--json` format.
    pub fn print(&self, json: bool) {
        if json {
            println!(
                "{}",
                json!({
                    "branch": self.branch,
                    "head_sha": self.head_sha,
                    "repository_url": self.repository_url,
                    "local": self.local,
                })
            );
        } else {
            println!(
                "Pushed {} ({}) to {}",
                self.branch,
                short_sha(&self.head_sha),
                self.repository_url
            );
        }
    }
}

impl From<PushResponse> for PushView {
    fn from(response: PushResponse) -> Self {
        Self {
            branch: response.branch,
            head_sha: response.head_sha,
            repository_url: response.repository_url,
            local: response.local,
        }
    }
}
