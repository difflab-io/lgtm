//! ReviewContextRepository groups reads and writes for review context.
use super::super::{entities::*, error::Result};
use sea_orm::sea_query::Expr;
use sea_orm::sea_query::ExprTrait;
use sea_orm::sea_query::OnConflict;
use sea_orm::{ActiveModelTrait, IntoActiveModel};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, TransactionTrait};
use std::sync::Arc;

pub enum ContextKey<'a> {
    Notes(&'a str),
    Branch {
        repository: &'a str,
        branch: &'a str,
    },
    PullRequest {
        host: &'a str,
        repository: &'a str,
        number: i64,
    },
}

#[derive(Clone)]
pub struct ReviewContextRepository<C = sea_orm::DatabaseConnection> {
    connection: Arc<C>,
}

impl<C> ReviewContextRepository<C> {
    pub(crate) fn new(connection: Arc<C>) -> Self {
        Self { connection }
    }
}

impl<C: ConnectionTrait + TransactionTrait> ReviewContextRepository<C> {
    pub async fn get(&self, id: &str) -> Result<Option<review_contexts::Model>> {
        Ok(review_contexts::Entity::find_by_id(id)
            .one(self.connection.as_ref())
            .await?)
    }

    pub async fn by_key(&self, key: ContextKey<'_>) -> Result<Option<review_contexts::Model>> {
        let query = review_contexts::Entity::find();
        let query = match key {
            ContextKey::Notes(path) => query
                .filter(review_contexts::Column::Kind.eq("notes"))
                .filter(review_contexts::Column::NotesPath.eq(path)),
            ContextKey::Branch { repository, branch } => query
                .filter(review_contexts::Column::Kind.eq("branch"))
                .filter(review_contexts::Column::Repository.eq(repository))
                .filter(review_contexts::Column::Branch.eq(branch)),
            ContextKey::PullRequest {
                host,
                repository,
                number,
            } => query
                .filter(review_contexts::Column::Kind.eq("pr"))
                .filter(review_contexts::Column::Host.eq(host))
                .filter(review_contexts::Column::Repository.eq(repository))
                .filter(review_contexts::Column::Number.eq(number)),
        };
        Ok(query.one(self.connection.as_ref()).await?)
    }

    pub async fn insert(&self, row: &review_contexts::Model) -> Result<()> {
        review_contexts::Entity::insert(row.clone().into_active_model().reset_all())
            .on_conflict(OnConflict::new().do_nothing().to_owned())
            .try_insert()
            .exec(self.connection.as_ref())
            .await?;
        Ok(())
    }

    pub async fn advance_revision(&self, id: &str, expected: Option<i64>) -> Result<bool> {
        let mut q = review_contexts::Entity::update_many()
            .col_expr(
                review_contexts::Column::Revision,
                Expr::col(review_contexts::Column::Revision).add(1),
            )
            .filter(review_contexts::Column::Id.eq(id));
        if let Some(revision) = expected {
            q = q.filter(review_contexts::Column::Revision.eq(revision));
        }
        Ok(q.exec(self.connection.as_ref()).await?.rows_affected == 1)
    }

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
            .filter(review_contexts::Column::Id.eq(&row.id))
            .exec(self.connection.as_ref())
            .await?;
        Ok(())
    }
}
