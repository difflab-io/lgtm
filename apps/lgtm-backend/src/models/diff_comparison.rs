//! Transient diff descriptors and selected-source data. No review ownership or persistence.
use super::{ReviewComparison, ReviewTarget};
use crate::{models as m, proto::lgtm::v1 as p};
use connectrpc::ConnectError;
use serde::{Deserialize, Serialize};

/// Transient content metadata. It is never persisted as a review record.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiffComparison {
    pub target: ReviewTarget,
    pub comparison: ReviewComparison,
    pub root: Option<String>,
    pub observation_token: String,
}

impl DiffComparison {
    // Public ------------------------------------------------------------------

    // RPC conversion ----------------------------------------------------------

    /// Convert the domain value to its RPC representation.
    pub(crate) fn into_proto(self) -> p::DiffDescriptor {
        let value = self;
        p::DiffDescriptor {
            target: Some(m::ReviewTarget::into_proto(value.target)).into(),
            comparison: Some(m::ReviewComparison::into_proto(value.comparison)).into(),
            root: value.root.unwrap_or_default(),
            observation_token: value.observation_token,
            ..Default::default()
        }
    }

    /// Validate and convert the RPC input to the domain model.
    pub(crate) fn from_proto(
        value: &p::DiffDescriptor,
    ) -> std::result::Result<m::DiffComparison, ConnectError> {
        Ok(m::DiffComparison {
            target: m::ReviewTarget::from_proto(&value.target)?,
            comparison: m::ReviewComparison::from_proto(&value.comparison)?.ok_or_else(|| {
                ConnectError::invalid_argument(
                    "Comparison descriptor is missing its pinned selection",
                )
            })?,
            root: (!value.root.is_empty()).then(|| value.root.clone()),
            observation_token: value.observation_token.clone(),
        })
    }
}
