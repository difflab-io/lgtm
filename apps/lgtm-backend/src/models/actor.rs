//! Logical human or agent attribution, independent of forge posting accounts.
use super::super::{database::models::identities, database::types::IdentityKind};
use super::{Result, invalid};
use serde::{Deserialize, Serialize};

/// An identity referenced by review records.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Actor {
    pub id: uuid::Uuid,
    pub name: String,
    #[serde(flatten)]
    pub kind: ActorKind,
}

/// Required attribution fields for each kind of actor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActorKind {
    Human { email: String },
    Agent { model: String, provider: String },
}

impl Actor {
    /// Validate the fields required by the selected identity kind.
    pub fn validate(&self) -> Result<()> {
        fn single(value: &str) -> bool {
            !value.trim().is_empty() && !value.contains(['\n', '\r'])
        }
        if !single(&self.name) {
            return Err(invalid("Identity requires a nonempty single-line name"));
        }
        match &self.kind {
            ActorKind::Human { email } if !single(email) || !email.contains('@') => {
                Err(invalid("Human identity requires an email"))
            }
            ActorKind::Agent { model, provider } if !single(model) || !single(provider) => {
                Err(invalid("Agent identity requires a model and provider"))
            }
            _ => Ok(()),
        }
    }

    /// Return the human email; agents have no email.
    pub fn email(&self) -> Option<&str> {
        match &self.kind {
            ActorKind::Human { email } => Some(email),
            _ => None,
        }
    }

    /// Return the agent model; humans have no model.
    pub fn model(&self) -> Option<&str> {
        match &self.kind {
            ActorKind::Agent { model, .. } => Some(model),
            _ => None,
        }
    }

    /// Format attribution for display and Markdown exports.
    pub fn label(&self) -> String {
        match &self.kind {
            ActorKind::Human { email } => format!("{} <{}>", self.name, email),
            ActorKind::Agent { model, provider } => format!("{} ({provider}/{model})", self.name),
        }
    }
}

impl From<&Actor> for identities::Model {
    fn from(actor: &Actor) -> Self {
        let (kind, email, model, provider) = match &actor.kind {
            ActorKind::Human { email } => (IdentityKind::Human, Some(email.clone()), None, None),
            ActorKind::Agent { model, provider } => (
                IdentityKind::Agent,
                None,
                Some(model.clone()),
                Some(provider.clone()),
            ),
        };
        Self {
            id: actor.id,
            name: actor.name.clone(),
            kind,
            email,
            model,
            provider,
            forge_account: String::new(),
        }
    }
}

impl TryFrom<identities::Model> for Actor {
    type Error = super::super::error::Error;

    fn try_from(row: identities::Model) -> Result<Self> {
        let kind = match row.kind {
            IdentityKind::Human => ActorKind::Human {
                email: row
                    .email
                    .ok_or_else(|| invalid("Human identity missing email"))?,
            },
            IdentityKind::Agent => ActorKind::Agent {
                model: row
                    .model
                    .ok_or_else(|| invalid("Agent identity missing model"))?,
                provider: row
                    .provider
                    .ok_or_else(|| invalid("Agent identity missing provider"))?,
            },
        };
        let actor = Self {
            id: row.id,
            name: row.name,
            kind,
        };
        actor.validate()?;
        Ok(actor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variants_require_their_own_fields() {
        let mut actor = Actor {
            id: uuid::Uuid::new_v4(),
            name: "Reviewer".into(),
            kind: ActorKind::Human {
                email: String::new(),
            },
        };
        assert!(actor.validate().is_err());
        actor.kind = ActorKind::Agent {
            model: "model".into(),
            provider: String::new(),
        };
        assert!(actor.validate().is_err());
        actor.kind = ActorKind::Agent {
            model: "model".into(),
            provider: "provider".into(),
        };
        assert!(actor.validate().is_ok());
        let row = identities::Model::from(&actor);
        assert_eq!(row.email, None);
        assert_eq!(row.model.as_deref(), Some("model"));
        assert_eq!(Actor::try_from(row).unwrap(), actor);
    }
}
