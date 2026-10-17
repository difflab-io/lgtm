use super::super::database::models::threads;
use super::super::{
    database as db,
    error::{Error, Result},
};
use super::identities::IdentityMap;
use super::{Actor, Anchor};
use sea_orm::{ConnectionTrait, TransactionTrait};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Thread {
    pub id: String,
    pub context_id: String,
    pub origin_session_id: Option<String>,
    pub author: Actor,
    pub anchor: Anchor,
    pub state: String,
    pub resolved_by: Option<Actor>,
    pub resolved_at: Option<String>,
    pub updated_by: Option<Actor>,
    pub updated_at: String,
    pub forge_thread_id: Option<String>,
    pub outdated: bool,
}

impl From<&Thread> for threads::Model {
    fn from(v: &Thread) -> Self {
        Self {
            id: v.id.clone(),
            context_id: v.context_id.clone(),
            origin_session_id: v.origin_session_id.clone(),
            author_id: v.author.id.clone(),
            anchor_kind: match v.anchor {
                Anchor::General => "general",
                Anchor::File { .. } => "file",
                Anchor::Line { .. } => "line",
            }
            .into(),
            file_path: match &v.anchor {
                Anchor::General => None,
                Anchor::File { path } | Anchor::Line { path, .. } => Some(path.clone()),
            },
            side: match &v.anchor {
                Anchor::Line { side, .. } => Some(side.clone()),
                _ => None,
            },
            reviewed_commit: match &v.anchor {
                Anchor::Line { commit, .. } => Some(commit.clone()),
                _ => None,
            },
            start_line: match &v.anchor {
                Anchor::Line { start, .. } => Some(*start),
                _ => None,
            },
            end_line: match &v.anchor {
                Anchor::Line { end, .. } => Some(*end),
                _ => None,
            },
            state: v.state.clone(),
            resolved_by_id: v.resolved_by.as_ref().map(|a| a.id.clone()),
            resolved_at: v.resolved_at.clone(),
            updated_by_id: v.updated_by.as_ref().map(|a| a.id.clone()),
            updated_at: v.updated_at.clone(),
            forge_thread_id: v.forge_thread_id.clone(),
            outdated: v.outdated,
        }
    }
}

impl Thread {
    fn with_identities(v: threads::Model, actors: &IdentityMap) -> Result<Self> {
        Ok(Thread {
            id: v.id,
            context_id: v.context_id,
            origin_session_id: v.origin_session_id,
            state: v.state,
            resolved_at: v.resolved_at,
            updated_at: v.updated_at,
            forge_thread_id: v.forge_thread_id,
            outdated: v.outdated,
            author: actors.get(&v.author_id)?,
            resolved_by: actors.optional(v.resolved_by_id.as_deref())?,
            updated_by: actors.optional(v.updated_by_id.as_deref())?,
            anchor: match v.anchor_kind.as_str() {
                "file" => Anchor::File {
                    path: v.file_path.unwrap_or_default(),
                },
                "line" => Anchor::Line {
                    path: v.file_path.unwrap_or_default(),
                    start: v.start_line.unwrap_or_default(),
                    end: v.end_line.unwrap_or_default(),
                    side: v.side.unwrap_or_default(),
                    commit: v.reviewed_commit.unwrap_or_default(),
                },
                _ => Anchor::General,
            },
        })
    }

    pub async fn from_db_models<C: ConnectionTrait + TransactionTrait>(
        rows: Vec<threads::Model>,
        db: &db::Database<C>,
    ) -> Result<Vec<Thread>> {
        let mut ids = HashSet::new();
        for r in &rows {
            ids.insert(r.author_id.clone());
            if let Some(id) = &r.resolved_by_id {
                ids.insert(id.clone());
            }
            if let Some(id) = &r.updated_by_id {
                ids.insert(id.clone());
            }
        }
        let actors = IdentityMap::load(ids, db).await?;
        rows.into_iter()
            .map(|r| Self::with_identities(r, &actors))
            .collect()
    }

    pub async fn from_db_model<C: ConnectionTrait + TransactionTrait>(
        row: threads::Model,
        db: &db::Database<C>,
    ) -> Result<Thread> {
        Self::from_db_models(vec![row], db)
            .await?
            .pop()
            .ok_or_else(|| Error::NotFound("Thread".into()))
    }
}
