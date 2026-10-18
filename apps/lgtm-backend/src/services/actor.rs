//! Batch actor loading for service-owned domain model conversions.
use super::super::models::Actor;
use super::super::{
    database as db,
    error::{Error, Result},
};
use sea_orm::{ConnectionTrait, TransactionTrait};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

pub(super) async fn load_actors<C: ConnectionTrait + TransactionTrait>(
    ids: HashSet<Uuid>,
    db: &db::Database<C>,
) -> Result<HashMap<Uuid, Actor>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    db.identities
        .get_many(ids.into_iter().collect())
        .await?
        .into_iter()
        .map(|row| Ok((row.id, Actor::try_from(row)?)))
        .collect()
}

pub(super) fn get_actor(actors: &HashMap<Uuid, Actor>, id: &Uuid) -> Result<Actor> {
    actors
        .get(id)
        .cloned()
        .ok_or_else(|| Error::NotFound(format!("Identity {id}")))
}
