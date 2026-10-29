use super::Anchor;
use serde::{Deserialize, Serialize};

/// A complete provider observation. Current coordinates never replace original evidence.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForgeObservation {
    pub base: String,
    pub head: String,
    pub outdated: bool,
    pub current: Option<Anchor>,
}

impl ForgeObservation {
    // Public ------------------------------------------------------------------

    pub(crate) fn from_proto(
        value: &crate::proto::lgtm::v1::ForgeObservation,
    ) -> Result<Self, connectrpc::ConnectError> {
        let comparison = super::ReviewComparison::PullRequest {
            base_commit: value.base.clone(),
            head_commit: value.head.clone(),
        };
        comparison
            .validate()
            .map_err(connectrpc::ConnectError::from)?;
        Ok(Self {
            base: value.base.clone(),
            head: value.head.clone(),
            outdated: value.outdated,
            current: if value.current.is_set() {
                Some(Anchor::from_proto(&value.current)?)
            } else {
                None
            },
        })
    }
}
