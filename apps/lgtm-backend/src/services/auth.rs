//! Resolve the supplied requester identity. Authentication and authorization are future work.
use super::super::database::Database;
use super::super::{
    error::{Error, Result, invalid},
    models::Actor,
};
use connectrpc::RequestContext;

#[derive(Clone)]
pub struct AuthService {
    database: Database,
}

impl AuthService {
    pub fn new(database: Database) -> Self {
        Self { database }
    }

    /// Resolve an explicitly supplied identity UUID from gRPC/Connect request metadata.
    /// This identifies the claimed requester; it does not authenticate that claim.
    pub async fn requester(&self, request: &RequestContext) -> Result<Actor> {
        tracing::debug!("Resolving requester identity");
        let value = request
            .header("x-lgtm-identity")
            .ok_or_else(|| invalid("Request requires x-lgtm-identity metadata"))?;
        let id = value
            .to_str()
            .map_err(|_| invalid("Identity metadata must be text"))?
            .trim();
        if id.is_empty() || request.headers().get_all("x-lgtm-identity").iter().count() != 1 {
            return Err(invalid("Request requires one nonempty identity UUID"));
        }
        self.identity(id).await
    }

    pub async fn ensure_identity(
        &self,
        name: &str,
        email: &str,
        model: Option<String>,
        provider: Option<String>,
    ) -> Result<Actor> {
        Actor::find_or_create(&self.database, name, email, model, provider).await
    }

    pub async fn identity(&self, id: &str) -> Result<Actor> {
        self.database
            .identities
            .get(id)
            .await?
            .map(Actor::from)
            .ok_or_else(|| Error::NotFound(format!("Identity {id}")))
    }

    pub(super) async fn validate_requester(&self, actor: &Actor) -> Result<Actor> {
        actor.validate()?;
        let current = self.identity(&actor.id).await?;
        if current.model != actor.model || current.provider != actor.provider {
            return Err(invalid(
                "Model/provider identify a different actor; create or select that identity",
            ));
        }
        Ok(current)
    }
}
