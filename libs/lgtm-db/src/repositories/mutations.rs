//! Durable results for safe local mutation retries. Submission receipts remain on sessions/comments.
use super::super::{entities::mutation_receipts, error::Result};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, IntoActiveModel, QueryFilter,
    TransactionTrait,
};
use std::sync::Arc;

#[derive(Clone)]
pub struct MutationRepository<C = sea_orm::DatabaseConnection> {
    connection: Arc<C>,
}

impl<C: ConnectionTrait + TransactionTrait> MutationRepository<C> {
    /// Get the result for this actor's key in exactly one conversation.
    pub async fn get_result(
        &self,
        context: &uuid::Uuid,
        actor: &uuid::Uuid,
        key: &str,
    ) -> Result<Option<mutation_receipts::Model>> {
        Ok(
            mutation_receipts::Entity::find_by_id((*context, *actor, key.to_owned()))
                .one(self.connection.as_ref())
                .await?,
        )
    }

    /// An unresolved remote intent blocks another write to the same object and review submission.
    pub async fn get_pending(
        &self,
        context: &uuid::Uuid,
        object: Option<&uuid::Uuid>,
    ) -> Result<bool> {
        let mut query = mutation_receipts::Entity::find()
            .filter(mutation_receipts::Column::ContextId.eq(*context))
            .filter(mutation_receipts::Column::Pending.eq(true));
        if let Some(object) = object {
            query = query.filter(mutation_receipts::Column::ObjectId.eq(*object));
        }
        Ok(query.one(self.connection.as_ref()).await?.is_some())
    }

    /// Replace only this exact saved intent while the caller holds the context lock.
    pub async fn update_result(
        &self,
        prior: &mutation_receipts::Model,
        result_json: &str,
        version: i64,
        pending: bool,
    ) -> Result<bool> {
        use sea_orm::sea_query::Expr;
        Ok(mutation_receipts::Entity::update_many()
            .col_expr(
                mutation_receipts::Column::ResultJson,
                Expr::value(result_json),
            )
            .col_expr(mutation_receipts::Column::Version, Expr::value(version))
            .col_expr(mutation_receipts::Column::Pending, Expr::value(pending))
            .filter(mutation_receipts::Column::ContextId.eq(prior.context_id))
            .filter(mutation_receipts::Column::ActorId.eq(prior.actor_id))
            .filter(mutation_receipts::Column::RequestKey.eq(&prior.request_key))
            .filter(mutation_receipts::Column::RequestHash.eq(&prior.request_hash))
            .filter(mutation_receipts::Column::ResultJson.eq(&prior.result_json))
            .exec(self.connection.as_ref())
            .await?
            .rows_affected
            == 1)
    }

    /// Commit with the domain mutation while holding the context write lock.
    pub async fn insert_result(&self, row: &mutation_receipts::Model) -> Result<()> {
        row.clone()
            .into_active_model()
            .reset_all()
            .insert(self.connection.as_ref())
            .await?;
        Ok(())
    }

    pub(crate) fn new(connection: Arc<C>) -> Self {
        Self { connection }
    }
}
