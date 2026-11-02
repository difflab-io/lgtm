//! Command outputs for `lgtm add`, `lgtm commit`, and `lgtm push`.

use serde_json::json;

use crate::{
    backend::proto::lgtm::v1::{AddResponse, CommitResponse, PushResponse},
    models::common::{Identity, short_sha},
};

/// Command output for `lgtm add`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddCommandOutput {
    pub staged_paths: Vec<String>,
    pub local: bool,
}

impl AddCommandOutput {
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

impl From<(AddResponse, bool)> for AddCommandOutput {
    fn from((response, local): (AddResponse, bool)) -> Self {
        Self {
            staged_paths: response.staged_paths,
            local,
        }
    }
}

/// Command output for `lgtm commit`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitCommandOutput {
    pub commit_sha: String,
    pub branch: String,
    pub author: Option<Identity>,
    pub local: bool,
}

impl CommitCommandOutput {
    // Public ------------------------------------------------------------------

    /// Print the commit summary in terminal or `--json` format.
    pub fn print(&self, json: bool) {
        if json {
            println!(
                "{}",
                json!({
                    "commit_sha": self.commit_sha,
                    "branch": self.branch,
                    "author": self.author.as_ref().map(Identity::to_json),
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
                .map_or_else(|| "unknown".to_owned(), Identity::display_label);
            println!(
                "Committed {} on {} ({mode}) by {author_label}",
                short_sha(&self.commit_sha),
                self.branch
            );
        }
    }
}

impl From<CommitResponse> for CommitCommandOutput {
    fn from(response: CommitResponse) -> Self {
        Self {
            commit_sha: response.commit_sha,
            branch: response.branch,
            author: response.author.as_option().map(Identity::from),
            local: response.local,
        }
    }
}

/// Command output for `lgtm push`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PushCommandOutput {
    pub branch: String,
    pub head_sha: String,
    pub repository_url: String,
    pub local: bool,
}

impl PushCommandOutput {
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

impl From<PushResponse> for PushCommandOutput {
    fn from(response: PushResponse) -> Self {
        Self {
            branch: response.branch,
            head_sha: response.head_sha,
            repository_url: response.repository_url,
            local: response.local,
        }
    }
}
