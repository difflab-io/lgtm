//! Batch identity resolution shared by domain model conversions.
use super::super::models::Actor;
use super::super::{
    database as db,
    error::{Error, Result},
};
use sea_orm::{ConnectionTrait, TransactionTrait};
use std::collections::{HashMap, HashSet};

pub(super) struct IdentityMap(HashMap<uuid::Uuid, Actor>);

impl IdentityMap {
    pub(super) async fn load<C: ConnectionTrait + TransactionTrait>(
        ids: HashSet<uuid::Uuid>,
        db: &db::Database<C>,
    ) -> Result<Self> {
        if ids.is_empty() {
            return Ok(Self(HashMap::new()));
        }
        let rows = db.identities.get_many(ids.into_iter().collect()).await?;
        Ok(Self(
            rows.into_iter()
                .map(|r| Ok((r.id, Actor::try_from(r)?)))
                .collect::<Result<_>>()?,
        ))
    }

    pub(super) fn get(&self, id: &uuid::Uuid) -> Result<Actor> {
        self.0
            .get(id)
            .cloned()
            .ok_or_else(|| Error::NotFound(format!("Identity {id}")))
    }

    pub(super) fn optional(&self, id: Option<&uuid::Uuid>) -> Result<Option<Actor>> {
        id.map(|id| self.get(id)).transpose()
    }
}
