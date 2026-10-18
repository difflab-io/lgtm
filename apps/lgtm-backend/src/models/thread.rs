use super::super::database::models::threads;
use super::super::error::Result;
use super::{Actor, Anchor};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Thread {
    pub id: uuid::Uuid,
    pub context_id: uuid::Uuid,
    pub origin_session_id: Option<uuid::Uuid>,
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

impl Thread {
    pub fn from_db_model(
        v: threads::Model,
        author: Actor,
        resolved_by: Option<Actor>,
        updated_by: Option<Actor>,
    ) -> Result<Self> {
        Ok(Thread {
            id: v.id,
            context_id: v.context_id,
            origin_session_id: v.origin_session_id,
            state: v.state,
            resolved_at: v.resolved_at,
            updated_at: v.updated_at,
            forge_thread_id: v.forge_thread_id,
            outdated: v.outdated,
            author,
            resolved_by,
            updated_by,
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
}

impl From<&Thread> for threads::Model {
    fn from(v: &Thread) -> Self {
        Self {
            id: v.id,
            context_id: v.context_id,
            origin_session_id: v.origin_session_id,
            author_id: v.author.id,
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
            resolved_by_id: v.resolved_by.as_ref().map(|a| a.id),
            resolved_at: v.resolved_at.clone(),
            updated_by_id: v.updated_by.as_ref().map(|a| a.id),
            updated_at: v.updated_at.clone(),
            forge_thread_id: v.forge_thread_id.clone(),
            outdated: v.outdated,
        }
    }
}
