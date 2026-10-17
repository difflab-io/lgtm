//! Resolve the supplied requester identity. Authentication and authorization are future work.
use super::super::database::Database;
use super::super::{database as db, database::models::identities, models::id};
use super::super::{
    error::{Error, Result, invalid},
    models::Actor,
};
use connectrpc::RequestContext;
use sea_orm::{ConnectionTrait, TransactionTrait};

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
        let id = uuid::Uuid::parse_str(id)
            .map_err(|_| invalid("Identity metadata must contain a UUID"))?;
        self.get_identity(&id).await
    }

    pub async fn ensure_human(&self, name: &str, email: &str) -> Result<Actor> {
        Self::find_or_create(
            &self.database,
            name,
            super::super::models::ActorKind::Human {
                email: email.trim().to_lowercase(),
            },
            "",
        )
        .await
    }

    pub async fn ensure_agent(&self, name: &str, model: &str, provider: &str) -> Result<Actor> {
        Self::find_or_create(
            &self.database,
            name,
            super::super::models::ActorKind::Agent {
                model: model.into(),
                provider: provider.into(),
            },
            "",
        )
        .await
    }

    pub async fn get_identity(&self, id: &uuid::Uuid) -> Result<Actor> {
        self.database
            .identities
            .get_by_id(id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Identity {id}")))?
            .try_into()
    }

    pub(super) async fn validate_requester(&self, actor: &Actor) -> Result<Actor> {
        actor.validate()?;
        let current = self.get_identity(&actor.id).await?;
        if std::mem::discriminant(&current.kind) != std::mem::discriminant(&actor.kind)
            || (actor.model().is_some() && current.kind != actor.kind)
        {
            return Err(invalid(
                "Model/provider identify a different actor; create or select that identity",
            ));
        }
        Ok(current)
    }

    pub(super) async fn find_or_create<D: ConnectionTrait + TransactionTrait>(
        conn: &db::Database<D>,
        name: &str,
        kind: super::super::models::ActorKind,
        forge_account: &str,
    ) -> Result<Actor> {
        use super::super::models::ActorKind;
        let actor = Actor {
            id: id(),
            name: name.trim().into(),
            kind,
        };
        actor.validate()?;
        async fn find<D: ConnectionTrait + TransactionTrait>(
            conn: &db::Database<D>,
            actor: &Actor,
            account: &str,
        ) -> db::error::Result<Option<identities::Model>> {
            if !account.is_empty() {
                return conn.identities.get_by_forge_account(account).await;
            }
            match &actor.kind {
                ActorKind::Human { email } => conn.identities.get_by_human_email(email).await,
                ActorKind::Agent { model, provider } => {
                    conn.identities
                        .get_by_agent(&actor.name, model, provider)
                        .await
                }
            }
        }
        if let Some(row) = find(conn, &actor, forge_account).await? {
            return row.try_into();
        }
        let mut row = identities::Model::from(&actor);
        row.forge_account = forge_account.into();
        conn.identities.insert(&row).await?;
        find(conn, &actor, forge_account)
            .await?
            .ok_or_else(|| Error::NotFound("Identity".into()))?
            .try_into()
    }
}
