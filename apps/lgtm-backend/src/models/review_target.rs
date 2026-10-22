//! Consumer intent for a local Git, directory or PR review.
use crate::{models as m, proto::lgtm::v1 as p};
use connectrpc::ConnectError;
use serde::{Deserialize, Serialize};

/// A location or forge address chosen by an API consumer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReviewTarget {
    LocalGit {
        path: String,
        branch: Option<String>,
    },
    Directory {
        path: String,
    },
    PullRequest {
        repository_url: String,
        number: i64,
    },
}

impl ReviewTarget {
    // Public ------------------------------------------------------------------

    // RPC conversion ----------------------------------------------------------

    /// Validate and convert the RPC input to the domain model.
    pub(crate) fn from_proto(
        v: &p::ReviewTarget,
    ) -> std::result::Result<m::ReviewTarget, ConnectError> {
        match &v.kind {
            Some(p::review_target::Kind::LocalGit(v)) => Ok(m::ReviewTarget::LocalGit {
                path: v.path.clone(),
                branch: (!v.branch.is_empty()).then(|| v.branch.clone()),
            }),
            Some(p::review_target::Kind::Directory(v)) => Ok(m::ReviewTarget::Directory {
                path: v.path.clone(),
            }),
            Some(p::review_target::Kind::PullRequest(v)) => Ok(m::ReviewTarget::PullRequest {
                repository_url: v.repository_url.clone(),
                number: v.number,
            }),
            None => Err(ConnectError::invalid_argument("Select a review target")),
        }
    }

    /// Convert the domain value to its RPC representation.
    pub(crate) fn into_proto(self) -> p::ReviewTarget {
        let value = self;
        match value {
            m::ReviewTarget::LocalGit { path, branch } => p::ReviewTarget {
                kind: Some(
                    p::LocalGitTarget {
                        path,
                        branch: branch.unwrap_or_default(),
                        ..Default::default()
                    }
                    .into(),
                ),
                ..Default::default()
            },
            m::ReviewTarget::Directory { path } => p::ReviewTarget {
                kind: Some(
                    p::DirectoryTarget {
                        path,
                        ..Default::default()
                    }
                    .into(),
                ),
                ..Default::default()
            },
            m::ReviewTarget::PullRequest {
                repository_url,
                number,
            } => p::ReviewTarget {
                kind: Some(
                    p::PullRequestTarget {
                        repository_url,
                        number,
                        ..Default::default()
                    }
                    .into(),
                ),
                ..Default::default()
            },
        }
    }
}
