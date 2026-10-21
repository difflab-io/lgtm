//! SessionRepository groups reads and writes for sessions.
use super::super::{entities::*, error::Result};
use sea_orm::{ActiveModelTrait, IntoActiveModel};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, TransactionTrait};
use sea_orm::{QueryOrder, QuerySelect, QueryTrait};
use std::sync::Arc;

/// Repository bound to the owning database connection or transaction.
#[derive(Clone)]
pub struct SessionRepository<C = sea_orm::DatabaseConnection> {
    connection: Arc<C>,
}

impl<C: ConnectionTrait + TransactionTrait> SessionRepository<C> {
    /// Page visible attempts by target metadata. Ordinary internal notes writers stay hidden.
    pub async fn get_review_page(
        &self,
        context: Option<uuid::Uuid>,
        path: Option<&str>,
        branch: Option<&str>,
        state: Option<common::SessionState>,
        after: Option<uuid::Uuid>,
        limit: u64,
    ) -> Result<Vec<sessions::Model>> {
        let mut query = sessions::Entity::find()
            .filter(sessions::Column::ComparisonKind.ne(common::ComparisonKind::Notes));
        if let Some(context) = context {
            query = query.filter(sessions::Column::ContextId.eq(context));
        }
        if let Some(path) = path {
            let locations = workspace_paths::Entity::find()
                .select_only()
                .column(workspace_paths::Column::ContextId)
                .filter(workspace_paths::Column::Path.eq(path));
            let mut contexts = review_contexts::Entity::find()
                .select_only()
                .column(review_contexts::Column::Id)
                .filter(review_contexts::Column::GitDir.is_not_null())
                .filter(review_contexts::Column::Id.in_subquery(locations.into_query()));
            if let Some(branch) = branch {
                contexts = contexts.filter(review_contexts::Column::Branch.eq(branch));
            }
            query = query.filter(sessions::Column::ContextId.in_subquery(contexts.into_query()));
        }
        if let Some(state) = state {
            query = query.filter(sessions::Column::State.eq(state));
        }
        if let Some(after) = after {
            query = query.filter(sessions::Column::Id.gt(after));
        }
        Ok(query
            .order_by_asc(sessions::Column::Id)
            .limit(limit.min(201))
            .all(self.connection.as_ref())
            .await?)
    }
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
            .filter(sessions::Column::State.eq(crate::entities::common::SessionState::Active))
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
            q = q.filter(sessions::Column::State.eq(crate::entities::common::SessionState::Active));
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

    /// Atomically claim or confirm the existing submission without changing its frozen payload.
    /// Expected status prevents concurrent submitters from sending the same review twice.
    pub async fn update_submission(
        &self,
        id: &uuid::Uuid,
        expected_status: &str,
        status: &str,
        error: Option<&str>,
        forge_review_id: Option<&str>,
    ) -> Result<bool> {
        Ok(sessions::Entity::update_many()
            .set(sessions::ActiveModel {
                submission_status: sea_orm::Set(status.to_owned()),
                submission_error: sea_orm::Set(error.map(str::to_owned)),
                forge_review_id: sea_orm::Set(forge_review_id.map(str::to_owned)),
                ..Default::default()
            })
            .filter(sessions::Column::Id.eq(*id))
            .filter(sessions::Column::SubmissionStatus.eq(expected_status))
            .exec(self.connection.as_ref())
            .await?
            .rows_affected
            == 1)
    }

    /// Resume only the same definitely rejected submission. Keep its body and provider review ID.
    pub async fn reopen_rejected_submission(&self, row: &sessions::Model) -> Result<bool> {
        Ok(sessions::Entity::update_many()
            .set(sessions::ActiveModel {
                state: sea_orm::Set(common::SessionState::Active),
                completed_at: sea_orm::Set(None),
                completed_by_id: sea_orm::Set(None),
                outcome: sea_orm::Set(None),
                request_key: sea_orm::Set(None),
                request_hash: sea_orm::Set(None),
                submission_status: sea_orm::Set("unsubmitted".into()),
                ..Default::default()
            })
            .filter(sessions::Column::Id.eq(row.id))
            .filter(sessions::Column::ContextId.eq(row.context_id))
            .filter(sessions::Column::State.eq(common::SessionState::Completed))
            .filter(sessions::Column::SubmissionStatus.eq("failed"))
            .filter(sessions::Column::RequestKey.eq(row.request_key.clone()))
            .filter(sessions::Column::RequestHash.eq(row.request_hash.clone()))
            .exec(self.connection.as_ref())
            .await?
            .rows_affected
            == 1)
    }

    /// Advance only an active, unfrozen PR review. Original comment anchors are stored separately.
    pub async fn update_pr_comparison(
        &self,
        row: &sessions::Model,
        base: &str,
        head: &str,
    ) -> Result<bool> {
        Ok(sessions::Entity::update_many()
            .set(sessions::ActiveModel {
                base_commit: sea_orm::Set(Some(base.into())),
                head_commit: sea_orm::Set(Some(head.into())),
                comparison_kind: sea_orm::Set(common::ComparisonKind::PullRequest),
                ..Default::default()
            })
            .filter(sessions::Column::Id.eq(row.id))
            .filter(sessions::Column::ContextId.eq(row.context_id))
            .filter(sessions::Column::State.eq(common::SessionState::Active))
            .filter(sessions::Column::SubmissionStatus.eq(&row.submission_status))
            .filter(sessions::Column::SubmissionStatus.is_in(["unsubmitted", "draft_failed"]))
            .filter(sessions::Column::BaseCommit.eq(row.base_commit.clone()))
            .filter(sessions::Column::HeadCommit.eq(row.head_commit.clone()))
            .filter(sessions::Column::ComparisonKind.is_in([
                common::ComparisonKind::Legacy,
                common::ComparisonKind::PullRequest,
            ]))
            .exec(self.connection.as_ref())
            .await?
            .rows_affected
            == 1)
    }

    /// Complete an active session without changing its creator or reviewed commits. Return false if no active row matches.
    pub async fn complete(&self, row: &sessions::Model) -> Result<bool> {
        Ok(sessions::Entity::update_many()
            .set(sessions::ActiveModel {
                state: sea_orm::Set(crate::entities::common::SessionState::Completed),
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
            .filter(sessions::Column::State.eq(crate::entities::common::SessionState::Active))
            .exec(self.connection.as_ref())
            .await?
            .rows_affected
            == 1)
    }
    pub(crate) fn new(connection: Arc<C>) -> Self {
        Self { connection }
    }
}
