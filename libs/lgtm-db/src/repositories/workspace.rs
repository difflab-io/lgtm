//! WorkspaceRepository groups reads and writes for workspace.
use super::super::{entities::*, error::Result};
use sea_orm::QueryOrder;
use sea_orm::sea_query::OnConflict;
use sea_orm::{ActiveModelTrait, IntoActiveModel};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, TransactionTrait};
use std::sync::Arc;

/// Repository bound to the owning database connection or transaction.
#[derive(Clone)]
pub struct WorkspaceRepository<C = sea_orm::DatabaseConnection> {
    connection: Arc<C>,
}

impl<C> WorkspaceRepository<C> {
    pub(crate) fn new(connection: Arc<C>) -> Self {
        Self { connection }
    }
}

impl<C: ConnectionTrait + TransactionTrait> WorkspaceRepository<C> {
    /// Get the canonical workspace paths associated with a context.
    pub async fn get_paths(&self, context_id: &uuid::Uuid) -> Result<Vec<workspace_paths::Model>> {
        Ok(workspace_paths::Entity::find()
            .filter(workspace_paths::Column::ContextId.eq(*context_id))
            .order_by_asc(workspace_paths::Column::Path)
            .all(self.connection.as_ref())
            .await?)
    }

    /// Insert a canonical repository natural key if it does not already exist.
    pub async fn insert_repository(&self, row: &repositories::Model) -> Result<()> {
        repositories::Entity::insert(row.clone().into_active_model().reset_all())
            .on_conflict(OnConflict::new().do_nothing().to_owned())
            .try_insert()
            .exec(self.connection.as_ref())
            .await?;
        Ok(())
    }

    /// Associate a canonical workspace path with its context if absent.
    pub async fn insert_path(&self, row: &workspace_paths::Model) -> Result<()> {
        workspace_paths::Entity::insert(row.clone().into_active_model().reset_all())
            .on_conflict(OnConflict::new().do_nothing().to_owned())
            .try_insert()
            .exec(self.connection.as_ref())
            .await?;
        Ok(())
    }
}
