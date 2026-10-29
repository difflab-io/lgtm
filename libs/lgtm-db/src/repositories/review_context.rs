//! ReviewContextRepository groups reads and writes for review context.
use super::super::{entities::*, error::Result};
use sea_orm::sea_query::Expr;
use sea_orm::sea_query::ExprTrait;
use sea_orm::sea_query::OnConflict;
use sea_orm::{ActiveModelTrait, IntoActiveModel};
use sea_orm::{
    ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QuerySelect, QueryTrait,
    TransactionTrait,
};
use std::sync::Arc;

/// Borrowed local location used to identify notes or a Git branch.
pub enum LocalLocation<'a> {
    Notes(&'a str),
    Branch { git_dir: &'a str, branch: &'a str },
}

/// Borrowed context lookup key; remote contexts require a complete repository URL.
pub enum ContextKey<'a> {
    Local(LocalLocation<'a>),
    ForgeBacked {
        repository_url: &'a str,
        number: i64,
    },
}

/// Repository bound to the owning database connection or transaction.
#[derive(Clone)]
pub struct ReviewContextRepository<C = sea_orm::DatabaseConnection> {
    connection: Arc<C>,
}

impl<C: ConnectionTrait + TransactionTrait> ReviewContextRepository<C> {
    /// Find file notes at their original path or a confirmed moved-path association.
    pub async fn get_notes_for_path(&self, path: &str) -> Result<Option<review_contexts::Model>> {
        if let Some(original) = review_contexts::Entity::find()
            .filter(review_contexts::Column::NotesPath.eq(path))
            .one(self.connection.as_ref())
            .await?
        {
            return Ok(Some(original));
        }
        if let Some(current) = review_contexts::Entity::find()
            .filter(review_contexts::Column::CurrentNotesPath.eq(path))
            .one(self.connection.as_ref())
            .await?
        {
            return Ok(Some(current));
        }
        let associated = workspace_paths::Entity::find()
            .select_only()
            .column(workspace_paths::Column::ContextId)
            .filter(workspace_paths::Column::Path.eq(path));
        let mut matches = review_contexts::Entity::find()
            .filter(review_contexts::Column::NotesPath.is_not_null())
            .filter(review_contexts::Column::Id.in_subquery(associated.into_query()))
            .limit(2)
            .all(self.connection.as_ref())
            .await?;
        if matches.len() > 1 {
            return Err(super::super::error::invalid(
                "Notes location has conflicting saved associations",
            ));
        }
        Ok(matches.pop())
    }

    /// Get a record by its internal UUID; return None when absent or outside the selected context.
    pub async fn get_by_id(&self, id: &uuid::Uuid) -> Result<Option<review_contexts::Model>> {
        Ok(review_contexts::Entity::find_by_id(*id)
            .one(self.connection.as_ref())
            .await?)
    }

    /// Get a local context by notes path or Git directory/branch, or a forge-backed context by repository URL/number.
    pub async fn get_by_key(&self, key: ContextKey<'_>) -> Result<Option<review_contexts::Model>> {
        let query = review_contexts::Entity::find();
        let query = match key {
            ContextKey::Local(LocalLocation::Notes(path)) => query
                .filter(
                    review_contexts::Column::Kind.eq(crate::entities::common::ContextKind::Local),
                )
                .filter(review_contexts::Column::NotesPath.eq(path)),
            ContextKey::Local(LocalLocation::Branch { git_dir, branch }) => query
                .filter(
                    review_contexts::Column::Kind.eq(crate::entities::common::ContextKind::Local),
                )
                .filter(review_contexts::Column::GitDir.eq(git_dir))
                .filter(review_contexts::Column::Branch.eq(branch)),
            ContextKey::ForgeBacked {
                repository_url,
                number,
            } => query
                .filter(
                    review_contexts::Column::Kind
                        .eq(crate::entities::common::ContextKind::ForgeBacked),
                )
                .filter(review_contexts::Column::RepositoryUrl.eq(repository_url))
                .filter(review_contexts::Column::Number.eq(number)),
        };
        Ok(query.one(self.connection.as_ref()).await?)
    }

    /// Insert a new record. Constraint violations are returned as database errors.
    pub async fn insert(&self, row: &review_contexts::Model) -> Result<()> {
        review_contexts::Entity::insert(row.clone().into_active_model().reset_all())
            .on_conflict(OnConflict::new().do_nothing().to_owned())
            .try_insert()
            .exec(self.connection.as_ref())
            .await?;
        Ok(())
    }

    /// Serialize mutation retries before comparing the saved version. No version changes here.
    pub async fn lock_for_write(&self, id: &uuid::Uuid) -> Result<bool> {
        Ok(review_contexts::Entity::update_many()
            .col_expr(
                review_contexts::Column::Revision,
                sea_orm::sea_query::Expr::col(review_contexts::Column::Revision),
            )
            .filter(review_contexts::Column::Id.eq(*id))
            .exec(self.connection.as_ref())
            .await?
            .rows_affected
            == 1)
    }

    /// Advance the revision, optionally requiring the expected value.
    pub async fn advance_revision(&self, id: &uuid::Uuid, expected: Option<i64>) -> Result<bool> {
        let mut q = review_contexts::Entity::update_many()
            .col_expr(
                review_contexts::Column::Revision,
                Expr::col(review_contexts::Column::Revision).add(1),
            )
            .filter(review_contexts::Column::Id.eq(*id));
        if let Some(revision) = expected {
            q = q.filter(review_contexts::Column::Revision.eq(revision));
        }
        Ok(q.exec(self.connection.as_ref()).await?.rows_affected == 1)
    }

    /// Update only the latest refresh metadata, preserving context identity and its natural key.
    pub async fn update_refresh(&self, row: &review_contexts::Model) -> Result<()> {
        review_contexts::Entity::update_many()
            .col_expr(
                review_contexts::Column::LastRefreshedAt,
                Expr::value(row.last_refreshed_at.clone()),
            )
            .col_expr(
                review_contexts::Column::RefreshedHead,
                Expr::value(row.refreshed_head.clone()),
            )
            .col_expr(
                review_contexts::Column::RefreshedBase,
                Expr::value(row.refreshed_base.clone()),
            )
            .col_expr(
                review_contexts::Column::RefreshStatus,
                Expr::value(row.refresh_status.clone()),
            )
            .col_expr(
                review_contexts::Column::RefreshError,
                Expr::value(row.refresh_error.clone()),
            )
            .col_expr(
                review_contexts::Column::ForgeState,
                Expr::value(row.forge_state),
            )
            .filter(review_contexts::Column::Id.eq(row.id))
            .exec(self.connection.as_ref())
            .await?;
        Ok(())
    }

    /// Local marks are per reviewer and survive review attempts; callers compare source identities.
    pub async fn get_file_reviews(
        &self,
        context: &uuid::Uuid,
        actor: &uuid::Uuid,
    ) -> Result<Vec<(String, String)>> {
        use crate::entities::file_reviews;
        Ok(file_reviews::Entity::find()
            .filter(file_reviews::Column::ContextId.eq(*context))
            .filter(file_reviews::Column::ActorId.eq(*actor))
            .all(self.connection.as_ref())
            .await?
            .into_iter()
            .map(|row| (row.path, row.content_identity))
            .collect())
    }

    /// Set an idempotent local file mark without changing the conversation revision.
    pub async fn set_file_reviewed(
        &self,
        context: &uuid::Uuid,
        actor: &uuid::Uuid,
        path: &str,
        content_identity: Option<&str>,
    ) -> Result<()> {
        use crate::entities::file_reviews;
        use sea_orm::{Set, sea_query::OnConflict};
        if let Some(identity) = content_identity {
            file_reviews::Entity::insert(file_reviews::ActiveModel {
                context_id: Set(*context),
                actor_id: Set(*actor),
                path: Set(path.into()),
                content_identity: Set(identity.into()),
            })
            .on_conflict(
                OnConflict::columns([
                    file_reviews::Column::ContextId,
                    file_reviews::Column::ActorId,
                    file_reviews::Column::Path,
                ])
                .update_column(file_reviews::Column::ContentIdentity)
                .to_owned(),
            )
            .exec(self.connection.as_ref())
            .await?;
        } else {
            file_reviews::Entity::delete_many()
                .filter(file_reviews::Column::ContextId.eq(*context))
                .filter(file_reviews::Column::ActorId.eq(*actor))
                .filter(file_reviews::Column::Path.eq(path))
                .exec(self.connection.as_ref())
                .await?;
        }
        Ok(())
    }

    pub(crate) fn new(connection: Arc<C>) -> Self {
        Self { connection }
    }
}
