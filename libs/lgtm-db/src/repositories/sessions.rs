//! SessionRepository groups reads and writes for sessions.
use super::super::{entities::*, error::Result};
use sea_orm::QueryOrder;
use sea_orm::{ActiveModelTrait, IntoActiveModel};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, TransactionTrait};
use std::sync::Arc;

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
    pub async fn get(&self, id: &str) -> Result<Option<sessions::Model>> {
        Ok(sessions::Entity::find_by_id(id)
            .one(self.connection.as_ref())
            .await?)
    }

    pub async fn active(&self, context: &str) -> Result<Option<sessions::Model>> {
        Ok(sessions::Entity::find()
            .filter(sessions::Column::ContextId.eq(context))
            .filter(sessions::Column::State.eq("active"))
            .one(self.connection.as_ref())
            .await?)
    }

    pub async fn completion_by_key(
        &self,
        context: &str,
        key: &str,
    ) -> Result<Option<sessions::Model>> {
        Ok(sessions::Entity::find()
            .filter(sessions::Column::ContextId.eq(context))
            .filter(sessions::Column::RequestKey.eq(key))
            .one(self.connection.as_ref())
            .await?)
    }

    pub async fn list(
        &self,
        context: Option<&str>,
        active_only: bool,
    ) -> Result<Vec<sessions::Model>> {
        let mut q = sessions::Entity::find();
        if let Some(c) = context {
            q = q.filter(sessions::Column::ContextId.eq(c));
        }
        if active_only {
            q = q.filter(sessions::Column::State.eq("active"));
        }
        Ok(q.order_by_asc(sessions::Column::CreatedAt)
            .order_by_asc(sessions::Column::Id)
            .all(self.connection.as_ref())
            .await?)
    }

    pub async fn insert(&self, row: &sessions::Model) -> Result<()> {
        row.clone()
            .into_active_model()
            .reset_all()
            .insert(self.connection.as_ref())
            .await?;
        Ok(())
    }

    pub async fn complete(&self, row: &sessions::Model) -> Result<bool> {
        Ok(sessions::Entity::update_many()
            .set(sessions::ActiveModel {
                state: sea_orm::Set("completed".to_owned()),
                completed_at: sea_orm::Set(row.completed_at.clone()),
                completed_by_id: sea_orm::Set(row.completed_by_id.clone()),
                outcome: sea_orm::Set(row.outcome.clone()),
                body: sea_orm::Set(row.body.clone()),
                submission_status: sea_orm::Set(row.submission_status.clone()),
                submission_error: sea_orm::Set(row.submission_error.clone()),
                request_key: sea_orm::Set(row.request_key.clone()),
                request_hash: sea_orm::Set(row.request_hash.clone()),
                ..Default::default()
            })
            .filter(sessions::Column::Id.eq(&row.id))
            .filter(sessions::Column::ContextId.eq(&row.context_id))
            .filter(sessions::Column::State.eq("active"))
            .exec(self.connection.as_ref())
            .await?
            .rows_affected
            == 1)
    }
}
