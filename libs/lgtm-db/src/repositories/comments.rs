//! CommentRepository groups reads and writes for comments.
use super::super::{entities::*, error::Result};
use sea_orm::QueryOrder;
use sea_orm::{ActiveModelTrait, IntoActiveModel};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, TransactionTrait};
use std::sync::Arc;

/// Repository bound to the owning database connection or transaction.
#[derive(Clone)]
pub struct CommentRepository<C = sea_orm::DatabaseConnection> {
    connection: Arc<C>,
}

impl<C> CommentRepository<C> {
    pub(crate) fn new(connection: Arc<C>) -> Self {
        Self { connection }
    }
}

impl<C: ConnectionTrait + TransactionTrait> CommentRepository<C> {
    /// Get a thread UUID within the selected context.
    pub async fn get_thread(
        &self,
        context: &uuid::Uuid,
        id: &uuid::Uuid,
    ) -> Result<Option<threads::Model>> {
        Ok(threads::Entity::find_by_id(*id)
            .filter(threads::Column::ContextId.eq(*context))
            .one(self.connection.as_ref())
            .await?)
    }

    /// Get a thread by its context-scoped opaque remote ID.
    pub async fn get_thread_by_remote_id(
        &self,
        context: &uuid::Uuid,
        remote: &str,
    ) -> Result<Option<threads::Model>> {
        Ok(threads::Entity::find()
            .filter(threads::Column::ContextId.eq(*context))
            .filter(threads::Column::ForgeThreadId.eq(remote))
            .one(self.connection.as_ref())
            .await?)
    }

    /// Get the context’s threads in stable UUID order.
    pub async fn get_threads(&self, context: &uuid::Uuid) -> Result<Vec<threads::Model>> {
        Ok(threads::Entity::find()
            .filter(threads::Column::ContextId.eq(*context))
            .order_by_asc(threads::Column::Id)
            .all(self.connection.as_ref())
            .await?)
    }

    /// Get a record by its internal UUID; return None when absent or outside the selected context.
    pub async fn get_by_id(
        &self,
        context: &uuid::Uuid,
        id: &uuid::Uuid,
    ) -> Result<Option<comments::Model>> {
        Ok(comments::Entity::find_by_id(*id)
            .filter(comments::Column::ContextId.eq(*context))
            .one(self.connection.as_ref())
            .await?)
    }

    /// Get a comment by context, remote object kind and opaque remote ID.
    pub async fn get_by_remote_id(
        &self,
        context: &uuid::Uuid,
        kind: &str,
        remote: &str,
    ) -> Result<Option<comments::Model>> {
        Ok(comments::Entity::find()
            .filter(comments::Column::ContextId.eq(*context))
            .filter(comments::Column::ForgeObjectKind.eq(kind))
            .filter(comments::Column::ForgeCommentId.eq(remote))
            .one(self.connection.as_ref())
            .await?)
    }

    /// Get matching records in stable display order.
    pub async fn get_all(&self, context: &uuid::Uuid) -> Result<Vec<comments::Model>> {
        Ok(comments::Entity::find()
            .filter(comments::Column::ContextId.eq(*context))
            .order_by_asc(comments::Column::ThreadId)
            .order_by_asc(comments::Column::Position)
            .all(self.connection.as_ref())
            .await?)
    }

    /// Get comments with remote IDs for complete-observation reconciliation.
    pub async fn get_remote(&self, context: &uuid::Uuid) -> Result<Vec<comments::Model>> {
        Ok(comments::Entity::find()
            .filter(comments::Column::ContextId.eq(*context))
            .filter(comments::Column::ForgeCommentId.is_not_null())
            .all(self.connection.as_ref())
            .await?)
    }

    /// Get unpublished comments originating in the selected session.
    pub async fn get_session_drafts(&self, session: &uuid::Uuid) -> Result<Vec<comments::Model>> {
        Ok(comments::Entity::find()
            .filter(comments::Column::OriginSessionId.eq(*session))
            .filter(comments::Column::Visibility.eq("draft"))
            .all(self.connection.as_ref())
            .await?)
    }

    /// Get the next reply position. Call while holding the context write lock.
    pub async fn get_next_position(&self, thread: &uuid::Uuid) -> Result<i64> {
        Ok(comments::Entity::find()
            .filter(comments::Column::ThreadId.eq(*thread))
            .order_by_desc(comments::Column::Position)
            .one(self.connection.as_ref())
            .await?
            .map_or(0, |c| c.position + 1))
    }

    /// Insert a thread with its immutable author and original anchor.
    pub async fn insert_thread(&self, row: &threads::Model) -> Result<()> {
        row.clone()
            .into_active_model()
            .reset_all()
            .insert(self.connection.as_ref())
            .await?;
        Ok(())
    }

    /// Insert a new record. Constraint violations are returned as database errors.
    pub async fn insert(&self, row: &comments::Model) -> Result<()> {
        row.clone()
            .into_active_model()
            .reset_all()
            .insert(self.connection.as_ref())
            .await?;
        Ok(())
    }

    /// Update resolution, applicability and remote linkage while preserving author, ownership and original anchor.
    pub async fn update_thread(&self, row: &threads::Model) -> Result<bool> {
        Ok(threads::Entity::update_many()
            .set(threads::ActiveModel {
                state: sea_orm::Set(row.state.clone()),
                resolved_by_id: sea_orm::Set(row.resolved_by_id),
                resolved_at: sea_orm::Set(row.resolved_at.clone()),
                updated_by_id: sea_orm::Set(row.updated_by_id),
                updated_at: sea_orm::Set(row.updated_at.clone()),
                forge_thread_id: sea_orm::Set(row.forge_thread_id.clone()),
                outdated: sea_orm::Set(row.outdated),
                ..Default::default()
            })
            .filter(threads::Column::Id.eq(row.id))
            .filter(threads::Column::ContextId.eq(row.context_id))
            .exec(self.connection.as_ref())
            .await?
            .rows_affected
            == 1)
    }

    /// Update content/publication/posting fields while preserving author, ownership, position and creation time.
    pub async fn update(&self, row: &comments::Model) -> Result<bool> {
        Ok(comments::Entity::update_many()
            .set(comments::ActiveModel {
                body: sea_orm::Set(row.body.clone()),
                visibility: sea_orm::Set(row.visibility.clone()),
                updated_at: sea_orm::Set(row.updated_at.clone()),
                forge_comment_id: sea_orm::Set(row.forge_comment_id.clone()),
                forge_object_kind: sea_orm::Set(row.forge_object_kind.clone()),
                forge_review_id: sea_orm::Set(row.forge_review_id.clone()),
                forge_body: sea_orm::Set(row.forge_body.clone()),
                forge_updated_at: sea_orm::Set(row.forge_updated_at.clone()),
                posting_account_id: sea_orm::Set(row.posting_account_id.clone()),
                posting_state: sea_orm::Set(row.posting_state),
                posting_error: sea_orm::Set(row.posting_error.clone()),
                posting_request_key: sea_orm::Set(row.posting_request_key.clone()),
                payload_hash: sea_orm::Set(row.payload_hash.clone()),
                remote_presence: sea_orm::Set(row.remote_presence.clone()),
                ..Default::default()
            })
            .filter(comments::Column::Id.eq(row.id))
            .filter(comments::Column::ContextId.eq(row.context_id))
            .exec(self.connection.as_ref())
            .await?
            .rows_affected
            == 1)
    }
}
