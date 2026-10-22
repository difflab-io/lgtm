//! A review attempt and the current conversation version.
use super::Session;
use super::{ReviewComparison, ReviewTarget};
use crate::{models as m, proto::lgtm::v1 as p};
use serde::{Deserialize, Serialize};

/// One session-backed public attempt plus its current conversation version.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Review {
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

    // RPC conversion ----------------------------------------------------------

    /// Convert the domain value to its RPC representation.
    pub(crate) fn into_proto(self) -> p::Review {
        let v = self;
        let target = m::ReviewTarget::into_proto(v.target);
        let comparison = v.comparison.map(m::ReviewComparison::into_proto);
        let completion = v.session.completion().cloned();
        p::Review {
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
