//! Named promotion operations; use in the same transaction as the draft insertion.
use super::super::{entities::note_promotions, error::Result};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, IntoActiveModel, QueryFilter,
    TransactionTrait,
};
use std::sync::Arc;
#[derive(Clone)]
pub struct PromotionRepository<C = sea_orm::DatabaseConnection> {
    connection: Arc<C>,
}
impl<C> PromotionRepository<C> {
    pub async fn get_source_target(
        &self,
        source: &uuid::Uuid,
        target: &uuid::Uuid,
    ) -> Result<Option<note_promotions::Model>>
    where
        C: ConnectionTrait + TransactionTrait,
    {
        Ok(note_promotions::Entity::find_by_id((*source, *target))
            .one(self.connection.as_ref())
            .await?)
    }
    pub async fn get_for_target(&self, target: &uuid::Uuid) -> Result<Vec<note_promotions::Model>>
    where
        C: ConnectionTrait + TransactionTrait,
    {
        Ok(note_promotions::Entity::find()
            .filter(note_promotions::Column::TargetSessionId.eq(*target))
            .all(self.connection.as_ref())
            .await?)
    }
    pub async fn insert(&self, row: &note_promotions::Model) -> Result<()>
    where
        C: ConnectionTrait + TransactionTrait,
    {
        row.clone()
            .into_active_model()
            .insert(self.connection.as_ref())
            .await?;
        Ok(())
    }
    pub(crate) fn new(connection: Arc<C>) -> Self {
        Self { connection }
    }
}
