//! Transient diff descriptors and selected-source data. No review ownership or persistence.
use crate::proto::lgtm::v1 as p;
use serde::{Deserialize, Serialize};

/// File metadata contains no downloaded file bodies.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiffFile {
    pub path: String,
    pub previous_path: String,
    pub status: String,
    pub identity: String,
}

impl DiffFile {
    // Public ------------------------------------------------------------------

    // RPC conversion ----------------------------------------------------------

    /// Convert the domain value to its RPC representation.
    pub(crate) fn into_proto(self) -> p::DiffFile {
        let value = self;
        p::DiffFile {
            path: value.path,
            previous_path: value.previous_path,
            status: value.status,
            identity: value.identity,
            ..Default::default()
        }
    }
}
