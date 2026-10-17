//! CommentRepository groups reads and writes for comments.
use super::super::{entities::*, error::Result};
use sea_orm::QueryOrder;
use sea_orm::{ActiveModelTrait, IntoActiveModel};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, TransactionTrait};
use std::sync::Arc;

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
    pub async fn get_thread(&self, context: &str, id: &str) -> Result<Option<threads::Model>> {
        Ok(threads::Entity::find_by_id(id)
            .filter(threads::Column::ContextId.eq(context))
            .one(self.connection.as_ref())
            .await?)
    }

    pub async fn get_thread_by_remote_id(
        &self,
        context: &str,
        remote: &str,
    ) -> Result<Option<threads::Model>> {
        Ok(threads::Entity::find()
            .filter(threads::Column::ContextId.eq(context))
            .filter(threads::Column::ForgeThreadId.eq(remote))
            .one(self.connection.as_ref())
            .await?)
    }

    pub async fn list_threads(&self, context: &str) -> Result<Vec<threads::Model>> {
        Ok(threads::Entity::find()
            .filter(threads::Column::ContextId.eq(context))
            .order_by_asc(threads::Column::Id)
            .all(self.connection.as_ref())
            .await?)
    }

    pub async fn get(&self, context: &str, id: &str) -> Result<Option<comments::Model>> {
        Ok(comments::Entity::find_by_id(id)
            .filter(comments::Column::ContextId.eq(context))
            .one(self.connection.as_ref())
            .await?)
    }

    pub async fn get_by_remote_id(
        &self,
        context: &str,
        kind: &str,
        remote: &str,
    ) -> Result<Option<comments::Model>> {
        Ok(comments::Entity::find()
            .filter(comments::Column::ContextId.eq(context))
            .filter(comments::Column::ForgeObjectKind.eq(kind))
            .filter(comments::Column::ForgeCommentId.eq(remote))
            .one(self.connection.as_ref())
            .await?)
    }

    pub async fn list(&self, context: &str) -> Result<Vec<comments::Model>> {
        Ok(comments::Entity::find()
            .filter(comments::Column::ContextId.eq(context))
            .order_by_asc(comments::Column::ThreadId)
            .order_by_asc(comments::Column::Position)
            .all(self.connection.as_ref())
            .await?)
    }

    pub async fn list_remote(&self, context: &str) -> Result<Vec<comments::Model>> {
        Ok(comments::Entity::find()
            .filter(comments::Column::ContextId.eq(context))
            .filter(comments::Column::ForgeCommentId.is_not_null())
            .all(self.connection.as_ref())
            .await?)
    }

    pub async fn drafts_for_session(&self, session: &str) -> Result<Vec<comments::Model>> {
        Ok(comments::Entity::find()
            .filter(comments::Column::OriginSessionId.eq(session))
            .filter(comments::Column::Visibility.eq("draft"))
            .all(self.connection.as_ref())
            .await?)
    }

    pub async fn next_position(&self, thread: &str) -> Result<i64> {
        Ok(comments::Entity::find()
            .filter(comments::Column::ThreadId.eq(thread))
            .order_by_desc(comments::Column::Position)
            .one(self.connection.as_ref())
            .await?
            .map_or(0, |c| c.position + 1))
    }

    pub async fn insert_thread(&self, row: &threads::Model) -> Result<()> {
        row.clone()
            .into_active_model()
            .reset_all()
            .insert(self.connection.as_ref())
            .await?;
        Ok(())
    }

    pub async fn insert(&self, row: &comments::Model) -> Result<()> {
        row.clone()
            .into_active_model()
            .reset_all()
            .insert(self.connection.as_ref())
            .await?;
        Ok(())
    }

    pub async fn update_thread(&self, row: &threads::Model) -> Result<bool> {
        Ok(threads::Entity::update_many()
            .set(threads::ActiveModel {
                state: sea_orm::Set(row.state.clone()),
                resolved_by_id: sea_orm::Set(row.resolved_by_id.clone()),
                resolved_at: sea_orm::Set(row.resolved_at.clone()),
                updated_by_id: sea_orm::Set(row.updated_by_id.clone()),
                updated_at: sea_orm::Set(row.updated_at.clone()),
                forge_thread_id: sea_orm::Set(row.forge_thread_id.clone()),
                outdated: sea_orm::Set(row.outdated),
                ..Default::default()
            })
            .filter(threads::Column::Id.eq(&row.id))
            .filter(threads::Column::ContextId.eq(&row.context_id))
            .exec(self.connection.as_ref())
            .await?
            .rows_affected
            == 1)
    }

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
                posting_state: sea_orm::Set(row.posting_state.clone()),
                posting_error: sea_orm::Set(row.posting_error.clone()),
                posting_request_key: sea_orm::Set(row.posting_request_key.clone()),
                payload_hash: sea_orm::Set(row.payload_hash.clone()),
                remote_presence: sea_orm::Set(row.remote_presence.clone()),
                ..Default::default()
            })
            .filter(comments::Column::Id.eq(&row.id))
            .filter(comments::Column::ContextId.eq(&row.context_id))
            .exec(self.connection.as_ref())
            .await?
            .rows_affected
            == 1)
    }
}
