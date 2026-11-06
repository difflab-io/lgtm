//! POC sync contracts and persistence. Each local data directory belongs to one account.
mod repository;
mod snapshot;
pub use repository::SyncRepository;
use serde::{Deserialize, Serialize};
pub use snapshot::Snapshot;
use uuid::Uuid;

/// Durable upload; its UUID and exact payload survive retries and restarts.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Push {
    pub operation_id: Uuid,
    pub expected_version: i64,
    pub snapshot: Snapshot,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Change {
    pub version: i64,
    pub snapshot: Snapshot,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Pull {
    pub cursor: i64,
    pub changes: Vec<Change>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Account {
    pub id: Uuid,
}
