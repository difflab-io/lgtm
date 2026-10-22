//! CommentRepository groups reads and writes for comments.
use super::super::{entities::*, error::Result};
use sea_orm::{ActiveModelTrait, IntoActiveModel};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, TransactionTrait};
use sea_orm::{QueryOrder, QuerySelect, QueryTrait};
use std::sync::Arc;

/// Repository bound to the owning database connection or transaction.
#[derive(Clone)]
pub struct CommentRepository<C = sea_orm::DatabaseConnection> {
    connection: Arc<C>,
}

impl<C: ConnectionTrait + TransactionTrait> CommentRepository<C> {
    /// Get only the immutable roots and location observations needed by a conversation page.
    pub async fn get_threads_by_ids(
        &self,
        context: &uuid::Uuid,
        ids: &[uuid::Uuid],
    ) -> Result<Vec<threads::Model>> {
        Ok(threads::Entity::find()
            .filter(threads::Column::ContextId.eq(*context))
            .filter(threads::Column::Id.is_in(ids.to_vec()))
            .order_by_asc(threads::Column::Id)
            .all(self.connection.as_ref())
            .await?)
    }
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

    /// Read a visible-file subset using confirmed current paths, without changing hidden discussions.
    pub async fn get_threads_for_paths(
        &self,
        context: &uuid::Uuid,
        paths: &[String],
    ) -> Result<Vec<threads::Model>> {
        Ok(threads::Entity::find()
            .filter(threads::Column::ContextId.eq(*context))
            .filter(
                sea_orm::Condition::any()
                    .add(threads::Column::CurrentPath.is_in(paths.to_vec()))
                    .add(
                        sea_orm::Condition::all()
                            .add(threads::Column::CurrentPath.is_null())
                            .add(
                                sea_orm::Condition::any()
                                    .add(threads::Column::SourcePath.is_in(paths.to_vec()))
                                    .add(
                                        sea_orm::Condition::all()
                                            .add(threads::Column::SourcePath.is_null())
                                            .add(threads::Column::FilePath.is_in(paths.to_vec())),
                                    ),
                            ),
                    ),
            )
            .order_by_asc(threads::Column::Id)
            .all(self.connection.as_ref())
            .await?)
    }

    /// Insert a thread with its immutable author and original anchor.
    pub async fn insert_thread(&self, row: &threads::Model) -> Result<()> {
        row.validate_source()?;
        row.validate_observation()?;
        row.clone()
            .into_active_model()
            .reset_all()
            .insert(self.connection.as_ref())
            .await?;
        Ok(())
    }

    /// Remove a thread after deleting its last comment in the same transaction.
    pub async fn delete_empty_thread(&self, context: &uuid::Uuid, id: &uuid::Uuid) -> Result<bool> {
        if comments::Entity::find()
            .filter(comments::Column::ThreadId.eq(*id))
            .one(self.connection.as_ref())
            .await?
            .is_some()
        {
            return Ok(false);
        }
        Ok(threads::Entity::delete_many()
            .filter(threads::Column::ContextId.eq(*context))
            .filter(threads::Column::Id.eq(*id))
            .exec(self.connection.as_ref())
            .await?
            .rows_affected
            == 1)
    }

    /// Update resolution, applicability and remote linkage while preserving author, ownership and original anchor.
    pub async fn update_thread(&self, row: &threads::Model) -> Result<bool> {
        row.validate_observation()?;
        Ok(threads::Entity::update_many()
            .set(threads::ActiveModel {
                state: sea_orm::Set(row.state.clone()),
                resolved_by_id: sea_orm::Set(row.resolved_by_id),
                resolved_at: sea_orm::Set(row.resolved_at.clone()),
                updated_by_id: sea_orm::Set(row.updated_by_id),
                updated_at: sea_orm::Set(row.updated_at.clone()),
                forge_thread_id: sea_orm::Set(row.forge_thread_id.clone()),
                current_path: sea_orm::Set(row.current_path.clone()),
                orphan_reason: sea_orm::Set(row.orphan_reason),
                forge_observation: sea_orm::Set(row.forge_observation.clone()),

                ..Default::default()
            })
            .filter(threads::Column::Id.eq(row.id))
            .filter(threads::Column::ContextId.eq(row.context_id))
            .exec(self.connection.as_ref())
            .await?
            .rows_affected
            == 1)
    }

    /// Read bounded conversation records in thread/reply order. A cursor never changes visibility.
    pub async fn get_comment_page(
        &self,
        context: &uuid::Uuid,
        paths: Option<&[String]>,
        include_drafts: bool,
        after: Option<(uuid::Uuid, i64)>,
        limit: u64,
    ) -> Result<Vec<comments::Model>> {
        let mut query = comments::Entity::find().filter(comments::Column::ContextId.eq(*context));
        if !include_drafts {
            query = query.filter(comments::Column::Visibility.eq("published"));
        }
        if let Some(paths) = paths {
            let visible = threads::Entity::find()
                .select_only()
                .column(threads::Column::Id)
                .filter(threads::Column::ContextId.eq(*context))
                .filter(
                    sea_orm::Condition::any()
                        .add(threads::Column::CurrentPath.is_in(paths.to_vec()))
                        .add(
                            sea_orm::Condition::all()
                                .add(threads::Column::CurrentPath.is_null())
                                .add(
                                    sea_orm::Condition::any()
                                        .add(threads::Column::SourcePath.is_in(paths.to_vec()))
                                        .add(
                                            sea_orm::Condition::all()
                                                .add(threads::Column::SourcePath.is_null())
                                                .add(
                                                    threads::Column::FilePath.is_in(paths.to_vec()),
                                                ),
                                        ),
                                ),
                        ),
                );
            query = query.filter(comments::Column::ThreadId.in_subquery(visible.into_query()));
        }
        if let Some((thread, position)) = after {
            query = query.filter(
                sea_orm::Condition::any()
                    .add(comments::Column::ThreadId.gt(thread))
                    .add(
                        sea_orm::Condition::all()
                            .add(comments::Column::ThreadId.eq(thread))
                            .add(comments::Column::Position.gt(position)),
                    ),
            );
        }
        Ok(query
            .order_by_asc(comments::Column::ThreadId)
            .order_by_asc(comments::Column::Position)
            .limit(limit.min(501))
            .all(self.connection.as_ref())
            .await?)
    }

    /// Get a record by its internal UUID; return None when absent or outside the selected context.
    pub async fn get_comment_by_id(
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
    pub async fn get_comment_by_remote_id(
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
    pub async fn get_comments(&self, context: &uuid::Uuid) -> Result<Vec<comments::Model>> {
        Ok(comments::Entity::find()
            .filter(comments::Column::ContextId.eq(*context))
            .order_by_asc(comments::Column::ThreadId)
            .order_by_asc(comments::Column::Position)
            .all(self.connection.as_ref())
            .await?)
    }

    /// Get comments with remote IDs for complete-observation reconciliation.
    pub async fn get_remote_comments(&self, context: &uuid::Uuid) -> Result<Vec<comments::Model>> {
        Ok(comments::Entity::find()
            .filter(comments::Column::ContextId.eq(*context))
            .filter(comments::Column::ForgeCommentId.is_not_null())
            .all(self.connection.as_ref())
            .await?)
    }

    /// Get unpublished comments originating in the selected session.
    pub async fn get_session_draft_comments(
        &self,
        session: &uuid::Uuid,
    ) -> Result<Vec<comments::Model>> {
        Ok(comments::Entity::find()
            .filter(comments::Column::OriginSessionId.eq(*session))
            .filter(comments::Column::Visibility.eq("draft"))
            .all(self.connection.as_ref())
            .await?)
    }

    /// Get the next reply position. Call while holding the context write lock.
    pub async fn get_next_reply_position(&self, thread: &uuid::Uuid) -> Result<i64> {
        Ok(comments::Entity::find()
            .filter(comments::Column::ThreadId.eq(*thread))
            .order_by_desc(comments::Column::Position)
            .one(self.connection.as_ref())
            .await?
            .map_or(0, |c| c.position + 1))
    }

    /// Insert a new record. Constraint violations are returned as database errors.
    pub async fn insert_comment(&self, row: &comments::Model) -> Result<()> {
        row.clone()
            .into_active_model()
            .reset_all()
            .insert(self.connection.as_ref())
            .await?;
        Ok(())
    }

    /// Delete a selected draft after service ownership and posting-state checks.
    pub async fn delete_draft_comment(
        &self,
        context: &uuid::Uuid,
        id: &uuid::Uuid,
    ) -> Result<bool> {
        Ok(comments::Entity::delete_many()
            .filter(comments::Column::ContextId.eq(*context))
            .filter(comments::Column::Id.eq(*id))
            .filter(comments::Column::Visibility.eq("draft"))
            .filter(comments::Column::ForgeCommentId.is_null())
            .exec(self.connection.as_ref())
            .await?
            .rows_affected
            == 1)
    }

    /// Forget one confirmed remote deletion within its owning conversation.
    /// Unposted local drafts are never eligible for this operation.
    pub async fn delete_remote_comment(
        &self,
        context: &uuid::Uuid,
        id: &uuid::Uuid,
    ) -> Result<bool> {
        Ok(comments::Entity::delete_many()
            .filter(comments::Column::ContextId.eq(*context))
            .filter(comments::Column::Id.eq(*id))
            .filter(comments::Column::ForgeCommentId.is_not_null())
            .exec(self.connection.as_ref())
            .await?
            .rows_affected
            == 1)
    }

    /// Update content/publication/posting fields while preserving author, ownership, position and creation time.
    pub async fn update_comment(&self, row: &comments::Model) -> Result<bool> {
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

                ..Default::default()
            })
            .filter(comments::Column::Id.eq(row.id))
            .filter(comments::Column::ContextId.eq(row.context_id))
            .exec(self.connection.as_ref())
            .await?
            .rows_affected
            == 1)
    }

    pub(crate) fn new(connection: Arc<C>) -> Self {
        Self { connection }
    }
}
