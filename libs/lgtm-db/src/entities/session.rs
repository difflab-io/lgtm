//! SeaORM entity for the `sessions` table.

use crate::entities::common::{ReviewOutcome, SessionState};
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// Author-scoped review session batching draft comments until publication.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "sessions")]
pub struct Model {
    /// Internal UUID primary key.
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    /// Foreign key referencing `reviews.id`.
    pub review_id: Uuid,
    /// Foreign key referencing `identities.id` for the session author.
    pub author_id: Uuid,
    /// Sequential session number within `review_id` (`1, 2, 3...`).
    pub number: i64,
    /// Lifecycle state (`Draft` or `Published`).
    pub state: SessionState,
    /// Verdict (`Comment`, `Reject`, `Approve`, or `Request`); `None` while `Draft`.
    pub outcome: Option<ReviewOutcome>,
    /// Optional top-level summary markdown body.
    #[sea_orm(column_type = "Text", nullable)]
    pub body: Option<String>,
    /// Opaque remote pull request review identifier when synced with a forge.
    #[sea_orm(column_type = "Text", nullable)]
    pub forge_id: Option<String>,
    /// Timestamp when the session transitioned from `Draft` to `Published`.
    pub published_at: Option<DateTimeUtc>,
    /// Row creation timestamp in UTC.
    pub created_at: DateTimeUtc,
    /// Last modification timestamp in UTC.
    pub updated_at: DateTimeUtc,
}

impl ActiveModelBehavior for ActiveModel {}
