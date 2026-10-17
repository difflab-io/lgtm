//! IdentityRepository groups reads and writes for identities.
use super::super::{entities::*, error::Result};
use sea_orm::sea_query::OnConflict;
use sea_orm::{ActiveModelTrait, IntoActiveModel};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, TransactionTrait};
use std::sync::Arc;

/// Repository bound to the owning database connection or transaction.
#[derive(Clone)]
pub struct IdentityRepository<C = sea_orm::DatabaseConnection> {
    connection: Arc<C>,
}

impl<C> IdentityRepository<C> {
    pub(crate) fn new(connection: Arc<C>) -> Self {
        Self { connection }
    }
}

impl<C: ConnectionTrait + TransactionTrait> IdentityRepository<C> {
    /// Get a record by its internal UUID; return None when absent or outside the selected context.
    pub async fn get_by_id(&self, id: &uuid::Uuid) -> Result<Option<identities::Model>> {
        Ok(identities::Entity::find_by_id(*id)
            .one(self.connection.as_ref())
            .await?)
    }

    /// Get identities for a batch of UUIDs. Missing IDs are omitted.
    pub async fn get_many(&self, ids: Vec<uuid::Uuid>) -> Result<Vec<identities::Model>> {
        Ok(identities::Entity::find()
            .filter(identities::Column::Id.is_in(ids))
            .all(self.connection.as_ref())
            .await?)
    }

    /// Get the identity bound to a scoped opaque forge account.
    pub async fn get_by_forge_account(&self, account: &str) -> Result<Option<identities::Model>> {
        Ok(identities::Entity::find()
            .filter(identities::Column::ForgeAccount.eq(account))
            .one(self.connection.as_ref())
            .await?)
    }

    /// Get a local human identity by normalized email.
    pub async fn get_by_human_email(&self, email: &str) -> Result<Option<identities::Model>> {
        Ok(identities::Entity::find()
            .filter(identities::Column::Email.eq(email))
            .filter(identities::Column::ForgeAccount.eq(""))
            .filter(identities::Column::Kind.eq(super::super::types::IdentityKind::Human))
            .one(self.connection.as_ref())
            .await?)
    }

    /// Get an agent by display name, model and provider.
    pub async fn get_by_agent(
        &self,
        name: &str,
        model: &str,
        provider: &str,
    ) -> Result<Option<identities::Model>> {
        Ok(identities::Entity::find()
            .filter(identities::Column::Name.eq(name))
            .filter(identities::Column::Model.eq(model))
            .filter(identities::Column::Provider.eq(provider))
            .filter(identities::Column::Kind.eq(super::super::types::IdentityKind::Agent))
            .one(self.connection.as_ref())
            .await?)
    }

    /// Insert a new record. Constraint violations are returned as database errors.
    pub async fn insert(&self, row: &identities::Model) -> Result<()> {
        identities::Entity::insert(row.clone().into_active_model().reset_all())
            .on_conflict(OnConflict::new().do_nothing().to_owned())
            .try_insert()
            .exec(self.connection.as_ref())
            .await?;
        Ok(())
    }
}
