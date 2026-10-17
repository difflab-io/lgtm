//! SessionRepository groups reads and writes for sessions.
use super::super::{entities::*, error::Result};
use sea_orm::QueryOrder;
use sea_orm::{ActiveModelTrait, IntoActiveModel};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, TransactionTrait};
use std::sync::Arc;

/// Repository bound to the owning database connection or transaction.
#[derive(Clone)]
pub struct SessionRepository<C = sea_orm::DatabaseConnection> {
    connection: Arc<C>,
}

impl<C> SessionRepository<C> {
    pub(crate) fn new(connection: Arc<C>) -> Self {
        Self { connection }
    }
}

impl<C: ConnectionTrait + TransactionTrait> SessionRepository<C> {
    /// Get a record by its internal UUID; return None when absent or outside the selected context.
    pub async fn get_by_id(&self, id: &uuid::Uuid) -> Result<Option<sessions::Model>> {
        Ok(sessions::Entity::find_by_id(*id)
            .one(self.connection.as_ref())
            .await?)
    }

    /// Get the context’s one active session, if any.
    pub async fn get_active(&self, context: &uuid::Uuid) -> Result<Option<sessions::Model>> {
        Ok(sessions::Entity::find()
            .filter(sessions::Column::ContextId.eq(*context))
            .filter(sessions::Column::State.eq(super::super::types::SessionState::Active))
            .one(self.connection.as_ref())
            .await?)
    }

    /// Get the completed session associated with a context-scoped retry key.
    pub async fn get_completion_by_key(
        &self,
        context: &uuid::Uuid,
        key: &str,
    ) -> Result<Option<sessions::Model>> {
        Ok(sessions::Entity::find()
            .filter(sessions::Column::ContextId.eq(*context))
            .filter(sessions::Column::RequestKey.eq(key))
            .one(self.connection.as_ref())
            .await?)
    }

    /// Get matching records in stable display order.
    pub async fn get_all(
        &self,
        context: Option<&uuid::Uuid>,
        active_only: bool,
    ) -> Result<Vec<sessions::Model>> {
        let mut q = sessions::Entity::find();
        if let Some(c) = context {
            q = q.filter(sessions::Column::ContextId.eq(*c));
        }
        if active_only {
            q = q.filter(sessions::Column::State.eq(super::super::types::SessionState::Active));
        }
        Ok(q.order_by_asc(sessions::Column::CreatedAt)
            .order_by_asc(sessions::Column::Id)
            .all(self.connection.as_ref())
            .await?)
    }

    /// Insert a new record. Constraint violations are returned as database errors.
    pub async fn insert(&self, row: &sessions::Model) -> Result<()> {
        row.clone()
            .into_active_model()
            .reset_all()
            .insert(self.connection.as_ref())
            .await?;
        Ok(())
    }

    /// Complete an active session without changing its creator or reviewed commits. Return false if no active row matches.
    pub async fn complete(&self, row: &sessions::Model) -> Result<bool> {
        Ok(sessions::Entity::update_many()
            .set(sessions::ActiveModel {
                state: sea_orm::Set(super::super::types::SessionState::Completed),
                completed_at: sea_orm::Set(row.completed_at.clone()),
                completed_by_id: sea_orm::Set(row.completed_by_id),
                outcome: sea_orm::Set(row.outcome.clone()),
                body: sea_orm::Set(row.body.clone()),
                submission_status: sea_orm::Set(row.submission_status.clone()),
                submission_error: sea_orm::Set(row.submission_error.clone()),
                request_key: sea_orm::Set(row.request_key.clone()),
                request_hash: sea_orm::Set(row.request_hash.clone()),
                ..Default::default()
            })
            .filter(sessions::Column::Id.eq(row.id))
            .filter(sessions::Column::ContextId.eq(row.context_id))
            .filter(sessions::Column::State.eq(super::super::types::SessionState::Active))
            .exec(self.connection.as_ref())
            .await?
            .rows_affected
            == 1)
    }
}
