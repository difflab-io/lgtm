//! An existing review or saved-file notes selected by an API client.
use crate::{models as m, proto::lgtm::v1 as p};
use connectrpc::ConnectError;
use serde::{Deserialize, Serialize};

/// Notes use a file location and do not require a visible review.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReviewScope {
    Review { review_id: uuid::Uuid },
    Notes { path: String },
}

impl ReviewScope {
    // Public ------------------------------------------------------------------

    // RPC conversion ----------------------------------------------------------

    /// Validate and convert the RPC input to the domain model.
    pub(crate) fn from_proto(
        v: &p::ReviewScope,
    ) -> std::result::Result<m::ReviewScope, ConnectError> {
        match &v.kind {
            Some(p::review_scope::Kind::ReviewId(v)) => Ok(m::ReviewScope::Review {
                review_id: uuid::Uuid::parse_str(v)
                    .map_err(|_| ConnectError::invalid_argument("Expected UUID"))?,
            }),
            Some(p::review_scope::Kind::Notes(v)) => Ok(m::ReviewScope::Notes {
                path: v.path.clone(),
            }),
            None => Err(ConnectError::invalid_argument(
                "Select a review or notes scope",
            )),
        }
    }
}
