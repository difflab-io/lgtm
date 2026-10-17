//! IdentityRepository groups reads and writes for identities.
use super::super::{entities::*, error::Result};
use sea_orm::sea_query::OnConflict;
use sea_orm::{ActiveModelTrait, IntoActiveModel};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, TransactionTrait};
use std::sync::Arc;

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
    pub async fn get(&self, id: &str) -> Result<Option<identities::Model>> {
        Ok(identities::Entity::find_by_id(id)
            .one(self.connection.as_ref())
            .await?)
    }

    pub async fn get_many(&self, ids: Vec<String>) -> Result<Vec<identities::Model>> {
        Ok(identities::Entity::find()
            .filter(identities::Column::Id.is_in(ids))
            .all(self.connection.as_ref())
            .await?)
    }

    pub async fn by_provider(&self, provider: &str) -> Result<Option<identities::Model>> {
        Ok(identities::Entity::find()
            .filter(identities::Column::Provider.eq(provider))
            .one(self.connection.as_ref())
            .await?)
    }

    pub async fn by_email(&self, email: &str, provider: &str) -> Result<Vec<identities::Model>> {
        Ok(identities::Entity::find()
            .filter(identities::Column::Email.eq(email))
            .filter(identities::Column::Provider.eq(provider))
            .all(self.connection.as_ref())
            .await?)
    }

    pub async fn by_key(
        &self,
        email: &str,
        model: &str,
        provider: &str,
    ) -> Result<Option<identities::Model>> {
        Ok(identities::Entity::find()
            .filter(identities::Column::Email.eq(email))
            .filter(identities::Column::Model.eq(model))
            .filter(identities::Column::Provider.eq(provider))
            .one(self.connection.as_ref())
            .await?)
    }

    pub async fn insert(&self, row: &identities::Model) -> Result<()> {
        identities::Entity::insert(row.clone().into_active_model().reset_all())
            .on_conflict(OnConflict::new().do_nothing().to_owned())
            .try_insert()
            .exec(self.connection.as_ref())
            .await?;
        Ok(())
    }
}
