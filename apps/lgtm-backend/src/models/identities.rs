//! Batch identity resolution shared by domain model conversions.
use super::super::{
    database as db,
    error::{Error, Result},
};
use super::Actor;
use sea_orm::{ConnectionTrait, TransactionTrait};
use std::collections::{HashMap, HashSet};

pub(super) struct IdentityMap(HashMap<String, Actor>);

impl IdentityMap {
    pub(super) async fn load<C: ConnectionTrait + TransactionTrait>(
        ids: HashSet<String>,
        db: &db::Database<C>,
    ) -> Result<Self> {
        if ids.is_empty() {
            return Ok(Self(HashMap::new()));
        }
        let rows = db.identities.get_many(ids.into_iter().collect()).await?;
        Ok(Self(
            rows.into_iter().map(|r| (r.id.clone(), r.into())).collect(),
        ))
    }

    pub(super) fn get(&self, id: &str) -> Result<Actor> {
        self.0
            .get(id)
            .cloned()
            .ok_or_else(|| Error::NotFound(format!("Identity {id}")))
    }

    pub(super) fn optional(&self, id: Option<&str>) -> Result<Option<Actor>> {
        id.map(|id| self.get(id)).transpose()
    }
}
