//! Saved source history, ordered within a session.
use super::super::{
    entities::{review_revisions, sessions},
    error::Result,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, IntoActiveModel, QueryFilter,
    QueryOrder, QuerySelect, QueryTrait, TransactionTrait,
};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct RevisionRepository<C = sea_orm::DatabaseConnection> {
    connection: Arc<C>,
}

impl<C: ConnectionTrait + TransactionTrait> RevisionRepository<C> {
    // Public ------------------------------------------------------------------

    pub(crate) fn new(connection: Arc<C>) -> Self {
        Self { connection }
    }

    pub async fn get_current(&self, session: &Uuid) -> Result<Option<review_revisions::Model>> {
        Ok(review_revisions::Entity::find()
            .filter(review_revisions::Column::SessionId.eq(*session))
            .order_by_desc(review_revisions::Column::Sequence)
            .one(self.connection.as_ref())
            .await?)
    }

    pub async fn get_context_history(
        &self,
        context: &Uuid,
    ) -> Result<Vec<review_revisions::Model>> {
        let sessions = sessions::Entity::find()
            .select_only()
            .column(sessions::Column::Id)
            .filter(sessions::Column::ContextId.eq(*context));
        Ok(review_revisions::Entity::find()
            .filter(review_revisions::Column::SessionId.in_subquery(sessions.into_query()))
            .order_by_asc(review_revisions::Column::CreatedAt)
            .order_by_asc(review_revisions::Column::Sequence)
            .all(self.connection.as_ref())
            .await?)
    }

    /// Call inside the transaction holding the session's context write lock.
    pub async fn insert(&self, revision: &review_revisions::Model) -> Result<()> {
        revision
            .clone()
            .into_active_model()
            .reset_all()
            .insert(self.connection.as_ref())
            .await?;
        Ok(())
    }
}
