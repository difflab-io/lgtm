//! Pinned revisions or working/index content selected for review.
use super::ComparisonKind;
use crate::error::{Result, invalid};
use crate::{models as m, proto::lgtm::v1 as p};
use connectrpc::ConnectError;
use serde::{Deserialize, Serialize};

/// Selected comparison. Active PR reviews follow provider updates; local and completed comparisons stay pinned.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReviewComparison {
    Working {
        base_commit: String,
    },
    Staged {
        base_commit: String,
    },
    Branch {
        base_commit: String,
        head_commit: String,
    },
    Directory,
    PullRequest {
        base_commit: String,
        head_commit: String,
    },
}

impl ReviewComparison {
    // Public ------------------------------------------------------------------

    pub fn kind(&self) -> ComparisonKind {
        match self {
            Self::Working { .. } => ComparisonKind::Working,
            Self::Staged { .. } => ComparisonKind::Staged,
            Self::Branch { .. } => ComparisonKind::Branch,
            Self::Directory => ComparisonKind::Directory,
            Self::PullRequest { .. } => ComparisonKind::PullRequest,
        }
    }

    pub fn validate(&self) -> Result<()> {
        let commits: Vec<&String> = match self {
            Self::Working { base_commit } | Self::Staged { base_commit } => vec![base_commit],
            Self::Branch {
                base_commit,
                head_commit,
            }
            | Self::PullRequest {
                base_commit,
                head_commit,
            } => vec![base_commit, head_commit],
            Self::Directory => vec![],
        };
        if commits.iter().any(|sha| {
            ![40, 64].contains(&sha.len()) || !sha.bytes().all(|b| b.is_ascii_hexdigit())
        }) {
            return Err(invalid("Comparison requires full commit identities"));
        }
        Ok(())
    }

    // RPC conversion ----------------------------------------------------------

    /// Validate and convert the RPC input to the domain model.
    pub(crate) fn from_proto(
        v: &p::ReviewComparison,
    ) -> std::result::Result<Option<m::ReviewComparison>, ConnectError> {
        let out = match &v.kind {
            Some(p::review_comparison::Kind::Working(v)) => m::ReviewComparison::Working {
                base_commit: v.base_commit.clone(),
            },
            Some(p::review_comparison::Kind::Staged(v)) => m::ReviewComparison::Staged {
                base_commit: v.base_commit.clone(),
            },
            Some(p::review_comparison::Kind::Branch(v)) => m::ReviewComparison::Branch {
                base_commit: v.base_commit.clone(),
                head_commit: v.head_commit.clone(),
            },
            Some(p::review_comparison::Kind::Directory(_)) => m::ReviewComparison::Directory,
            Some(p::review_comparison::Kind::PullRequest(v)) => m::ReviewComparison::PullRequest {
                base_commit: v.base_commit.clone(),
                head_commit: v.head_commit.clone(),
            },
            None => return Ok(None),
        };
        out.validate().map_err(ConnectError::from)?;
        Ok(Some(out))
    }

    /// Convert the domain value to its RPC representation.
    pub(crate) fn into_proto(self) -> p::ReviewComparison {
        let c = self;
        p::ReviewComparison {
            kind: Some(match c {
                m::ReviewComparison::Working { base_commit } => p::WorkingComparison {
                    base_commit,
                    ..Default::default()
                }
                .into(),
                m::ReviewComparison::Staged { base_commit } => p::StagedComparison {
                    base_commit,
                    ..Default::default()
                }
                .into(),
                m::ReviewComparison::Branch {
                    base_commit,
                    head_commit,
                } => p::BranchComparison {
                    base_commit,
                    head_commit,
                    ..Default::default()
                }
                .into(),
                m::ReviewComparison::Directory => p::DirectoryComparison::default().into(),
                m::ReviewComparison::PullRequest {
                    base_commit,
                    head_commit,
                } => p::PullRequestComparison {
                    base_commit,
                    head_commit,
                    ..Default::default()
                }
                .into(),
            }),
            ..Default::default()
        }
    }
}
