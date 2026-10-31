//! Repository for human and AI agent identities.

use crate::{
    entities::{
        common::IdentityKind,
        identity::{self, ActiveModel, Column, Entity, Model as Identity},
    },
    error::{DbError, Result},
    repositories::{normalize_optional, require_non_empty},
};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder, Set,
};
use std::sync::Arc;
use uuid::Uuid;

/// Persistence operations for `identities` rows.
#[derive(Clone, Debug)]
pub struct IdentitiesRepository<C> {
    pool: Arc<C>,
}

impl<C: ConnectionTrait> IdentitiesRepository<C> {
    /// Construct a repository sharing the given connection or transaction handle.
    pub(crate) fn new(pool: Arc<C>) -> Self {
        Self { pool }
    }

    // Public ------------------------------------------------------------------

    /// Fetch an identity by its internal UUID primary key.
    pub async fn get_identity(&self, id: Uuid) -> Result<Option<Identity>> {
        Ok(Entity::find_by_id(id).one(self.pool.as_ref()).await?)
    }

    /// Fetch an identity by its unique handle (`name`).
    pub async fn get_identity_by_name(&self, name: &str) -> Result<Option<Identity>> {
        let name = name.trim();
        if name.is_empty() {
            return Ok(None);
        }
        Ok(Entity::find()
            .filter(Column::Name.eq(name))
            .one(self.pool.as_ref())
            .await?)
    }

    /// Batch-fetch identities matching any of the supplied UUIDs, ordered by name.
    pub async fn get_identities_by_ids(&self, ids: &[Uuid]) -> Result<Vec<Identity>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        Ok(Entity::find()
            .filter(Column::Id.is_in(ids.iter().copied()))
            .order_by_asc(Column::Name)
            .all(self.pool.as_ref())
            .await?)
    }

    /// Create or update a human reviewer identity by unique `name`.
    pub async fn upsert_human(
        &self,
        name: &str,
        email: &str,
        forge_login: Option<&str>,
    ) -> Result<Identity> {
        let name = require_non_empty("identity name", name)?;
        let email = require_non_empty("human email", email)?;
        let forge_login = normalize_optional(forge_login.map(str::to_owned));
        let now = Utc::now();

        if let Some(existing) = self.get_identity_by_name(name).await? {
            if existing.kind != IdentityKind::Human {
                return Err(DbError::conflict(format!(
                    "identity '{name}' already exists as {}",
                    existing.kind
                )));
            }
            let mut active: ActiveModel = existing.into();
            active.email = Set(Some(email.to_owned()));
            active.forge_login = Set(forge_login);
            active.updated_at = Set(now);
            return active
                .update(self.pool.as_ref())
                .await
                .map_err(DbError::from_db_err);
        }

        let active = identity::ActiveModel {
            id: Set(Uuid::new_v4()),
            name: Set(name.to_owned()),
            kind: Set(IdentityKind::Human),
            email: Set(Some(email.to_owned())),
            provider: Set(None),
            model: Set(None),
            forge_login: Set(forge_login),
            created_at: Set(now),
            updated_at: Set(now),
        };
        active
            .insert(self.pool.as_ref())
            .await
            .map_err(DbError::from_db_err)
    }

    /// Create or update an AI agent identity by unique `name`.
    pub async fn upsert_agent(&self, name: &str, provider: &str, model: &str) -> Result<Identity> {
        let name = require_non_empty("identity name", name)?;
        let provider = require_non_empty("agent provider", provider)?;
        let model = require_non_empty("agent model", model)?;
        let now = Utc::now();

        if let Some(existing) = self.get_identity_by_name(name).await? {
            if existing.kind != IdentityKind::Agent {
                return Err(DbError::conflict(format!(
                    "identity '{name}' already exists as {}",
                    existing.kind
                )));
            }
            let mut active: ActiveModel = existing.into();
            active.provider = Set(Some(provider.to_owned()));
            active.model = Set(Some(model.to_owned()));
            active.updated_at = Set(now);
            return active
                .update(self.pool.as_ref())
                .await
                .map_err(DbError::from_db_err);
        }

        let active = identity::ActiveModel {
            id: Set(Uuid::new_v4()),
            name: Set(name.to_owned()),
            kind: Set(IdentityKind::Agent),
            email: Set(None),
            provider: Set(Some(provider.to_owned())),
            model: Set(Some(model.to_owned())),
            forge_login: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        };
        active
            .insert(self.pool.as_ref())
            .await
            .map_err(DbError::from_db_err)
    }
}
