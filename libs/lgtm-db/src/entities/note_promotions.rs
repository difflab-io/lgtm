//! Durable source attribution and retry identity for a promoted note.
use sea_orm::entity::prelude::*;
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "note_promotions")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    /// Original note UUID, retained even if its source is later deleted.
    pub source_comment_id: uuid::Uuid,
    #[sea_orm(primary_key, auto_increment = false)]
    /// Review session receiving this note, part of the idempotency key.
    pub target_session_id: uuid::Uuid,
    /// Created draft UUID; NULL is a durable deletion tombstone.
    pub target_comment_id: Option<uuid::Uuid>,
    /// Client retry key supplied for the promotion batch.
    pub request_key: String,
    /// Hash of the source body, author and selected target anchor.
    pub payload_hash: String,
}
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
