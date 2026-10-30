//! A retained pair of immutable Git commits. Repository storage belongs to the backend host.
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: Uuid,
    pub repository_id: Uuid,
    pub root: String,
    pub base_commit: String,
    pub head_commit: String,
}

impl Snapshot {
    // Public ------------------------------------------------------------------

    pub fn from_db_model(row: crate::database::models::review_revisions::Model) -> Self {
        Self {
            id: row.id,
            repository_id: row.repository_id,
            root: row.root,
            base_commit: row.base_commit,
            head_commit: row.head_commit,
        }
    }

    pub(crate) fn into_proto(self) -> crate::proto::lgtm::v1::ReviewSnapshot {
        crate::proto::lgtm::v1::ReviewSnapshot {
            id: self.id.to_string(),
            repository_id: self.repository_id.to_string(),
            root: self.root,
            base_commit: self.base_commit,
            head_commit: self.head_commit,
            ..Default::default()
        }
    }
}
