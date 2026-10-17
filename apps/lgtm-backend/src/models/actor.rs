use super::super::database::models::identities;
use super::super::{database as db, error::Error};
use super::id;
use super::{Result, invalid};
use sea_orm::{ConnectionTrait, TransactionTrait};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Actor {
    pub id: String,
    pub name: String,
    pub email: String,
    pub model: Option<String>,
    pub provider: Option<String>,
}

impl Actor {
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty()
            || !self.email.contains('@')
            || self.name.contains(['\n', '\r'])
            || self.email.contains(['\n', '\r'])
        {
            return Err(invalid("Identity requires a single-line name and email"));
        }
        if self
            .model
            .as_ref()
            .is_some_and(|m| m.trim().is_empty() || m.contains(['\n', '\r']))
        {
            return Err(invalid("Model must be a nonempty single-line value"));
        }
        Ok(())
    }

    pub fn label(&self) -> String {
        format!(
            "{} <{}>{}",
            self.name,
            self.email,
            self.model
                .as_ref()
                .map(|m| format!(" ({m})"))
                .unwrap_or_default()
        )
    }
}

impl From<&Actor> for identities::Model {
    fn from(v: &Actor) -> Self {
        Self {
            id: v.id.clone(),
            name: v.name.clone(),
            email: v.email.clone(),
            model: v.model.clone().unwrap_or_default(),
            provider: v.provider.clone().unwrap_or_default(),
        }
    }
}

impl From<identities::Model> for Actor {
    fn from(v: identities::Model) -> Self {
        Self {
            id: v.id,
            name: v.name,
            email: v.email,
            model: (!v.model.is_empty()).then_some(v.model),
            provider: (!v.provider.is_empty()).then_some(v.provider),
        }
    }
}

impl Actor {
    pub(crate) async fn find_or_create<D: ConnectionTrait + TransactionTrait>(
        conn: &db::Database<D>,
        name: &str,
        email: &str,
        model: Option<String>,
        provider: Option<String>,
    ) -> Result<Actor> {
        let actor = Actor {
            id: id(),
            name: name.trim().into(),
            email: email.trim().to_lowercase(),
            model,
            provider,
        };
        actor.validate()?;
        let provider_key = actor.provider.as_deref().unwrap_or_default();
        if provider_key.starts_with("forge:")
            && let Some(found) = conn.identities.by_provider(provider_key).await?
        {
            return Ok(found.into());
        }
        if actor.model.is_none() {
            let found = conn.identities.by_email(&actor.email, provider_key).await?;
            if found.len() > 1 || found.first().is_some_and(|a| !a.model.is_empty()) {
                return Err(invalid(
                    "Ambiguous identity; use --identity UUID or specify --model",
                ));
            }
        }
        conn.identities
            .insert(&identities::Model::from(&actor))
            .await?;
        let row = if provider_key.starts_with("forge:") {
            conn.identities.by_provider(provider_key).await?
        } else {
            conn.identities
                .by_key(
                    &actor.email,
                    actor.model.as_deref().unwrap_or_default(),
                    provider_key,
                )
                .await?
        };
        row.map(Actor::from)
            .ok_or_else(|| Error::NotFound("Identity".into()))
    }
}
