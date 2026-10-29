//! A review attempt and the current conversation version.
use super::{ComparisonKind, Context, ContextKey, LocalLocation, Session};
use super::{ReviewComparison, ReviewTarget};
use crate::{models as m, proto::lgtm::v1 as p};
use serde::{Deserialize, Serialize};

/// One session-backed public attempt plus its current conversation version.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Review {
    pub snapshot: Option<super::Snapshot>,
    pub revisions: Vec<super::Snapshot>,
    pub target: ReviewTarget,
    pub comparison: Option<ReviewComparison>,
    /// Latest complete provider comparison; active PR reviews follow it before further draft writes.
    pub observed_comparison: Option<ReviewComparison>,
    pub session: Session,
    pub version: i64,
    pub forge_state: Option<crate::database::types::ForgeState>,
}

impl Review {
    // Public ------------------------------------------------------------------

    /// Build the public review from its selected session and shared conversation context.
    pub fn from_session(context: Context, session: Session) -> Self {
        let target = match &context.key {
            ContextKey::Local {
                location: LocalLocation::Notes { path },
            } => ReviewTarget::Directory {
                path: context
                    .current_notes_path
                    .clone()
                    .unwrap_or_else(|| path.clone()),
            },
            ContextKey::Local {
                location: LocalLocation::Branch { git_dir, branch },
            } => ReviewTarget::LocalGit {
                branch: Some(branch.clone()),
                path: context
                    .paths
                    .first()
                    .cloned()
                    .unwrap_or_else(|| git_dir.clone()),
            },
            ContextKey::ForgeBacked {
                repository_url,
                number,
            } => ReviewTarget::PullRequest {
                repository_url: repository_url.as_str().into(),
                number: *number,
            },
        };
        let comparison_kind = session.comparison_kind;
        let comparison = match comparison_kind {
            ComparisonKind::Working => session
                .commits
                .base
                .clone()
                .map(|base_commit| ReviewComparison::Working { base_commit }),
            ComparisonKind::Staged => session
                .commits
                .base
                .clone()
                .map(|base_commit| ReviewComparison::Staged { base_commit }),
            ComparisonKind::Directory => Some(ReviewComparison::Directory),
            ComparisonKind::Branch | ComparisonKind::PullRequest => session
                .commits
                .base
                .clone()
                .zip(session.commits.head.clone())
                .map(|(base_commit, head_commit)| {
                    if comparison_kind == ComparisonKind::Branch {
                        ReviewComparison::Branch {
                            base_commit,
                            head_commit,
                        }
                    } else {
                        ReviewComparison::PullRequest {
                            base_commit,
                            head_commit,
                        }
                    }
                }),
            ComparisonKind::Notes => None,
        };
        Review {
            snapshot: None,
            revisions: vec![],
            target,
            comparison,
            observed_comparison: if context.key.is_forge_backed() {
                context
                    .refreshed_base
                    .clone()
                    .zip(context.refreshed_head.clone())
                    .map(|(base_commit, head_commit)| ReviewComparison::PullRequest {
                        base_commit,
                        head_commit,
                    })
            } else {
                None
            },
            session,
            version: context.revision,
            forge_state: context.forge_state,
        }
    }

    // RPC conversion ----------------------------------------------------------

    /// Convert the domain value to its RPC representation.
    pub(crate) fn into_proto(self) -> p::Review {
        let v = self;
        let target = m::ReviewTarget::into_proto(v.target);
        let comparison = v.comparison.map(m::ReviewComparison::into_proto);
        let completion = v.session.completion().cloned();
        p::Review {
            snapshot: v.snapshot.map(super::Snapshot::into_proto).into(),
            revisions: v
                .revisions
                .into_iter()
                .map(super::Snapshot::into_proto)
                .collect(),
            id: v.session.id.to_string(),
            target: Some(target).into(),
            comparison: comparison.into(),
            observed_comparison: v
                .observed_comparison
                .map(m::ReviewComparison::into_proto)
                .into(),
            state: v.session.state().into(),
            version: v.version.to_string(),
            forge_state: v
                .forge_state
                .map(|state| state.as_str().to_owned())
                .unwrap_or_default(),
            creator: Some(m::Actor::into_proto(v.session.creator)).into(),
            created_at: v.session.created_at,
            outcome: completion
                .as_ref()
                .map(|c| c.outcome.clone())
                .unwrap_or_default(),
            body: completion
                .and_then(|c| c.body)
                .or(v.session.draft_body)
                .unwrap_or_default(),
            submission_status: v.session.submission_status,
            submission_error: v.session.submission_error.unwrap_or_default(),
            forge_review_id: v.session.forge_review_id.unwrap_or_default(),
            ..Default::default()
        }
    }
}
