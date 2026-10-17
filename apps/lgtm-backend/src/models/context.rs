use super::super::{database::models::review_contexts, error::Result};
use super::{ContextKey, ContextKind};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Context {
    pub id: uuid::Uuid,
    pub key: ContextKey,
    pub paths: Vec<String>,
    pub revision: i64,
    pub last_refreshed_at: Option<String>,
    pub refreshed_head: Option<String>,
    #[serde(default)]
    pub refreshed_base: Option<String>,
    pub refresh_status: String,
    pub refresh_error: Option<String>,
}

impl Context {
    pub fn from_db_model(row: review_contexts::Model, paths: Vec<String>) -> Result<Context> {
        let key = match row.kind {
            ContextKind::Notes => ContextKey::Notes {
                path: row.notes_path.unwrap_or_default(),
            },
            ContextKind::Branch => ContextKey::Branch {
                repository: row.repository.unwrap_or_default(),
                branch: row.branch.unwrap_or_default(),
            },
            ContextKind::PullRequest => ContextKey::PullRequest {
                host: row.host.unwrap_or_default(),
                repository: row.repository.unwrap_or_default(),
                number: row.number.unwrap_or_default(),
            },
        };
        Ok(Context {
            id: row.id,
            key,
            paths,
            revision: row.revision,
            last_refreshed_at: row.last_refreshed_at,
            refreshed_head: row.refreshed_head,
            refreshed_base: row.refreshed_base,
            refresh_status: row.refresh_status,
            refresh_error: row.refresh_error,
        })
    }
}

impl From<&Context> for review_contexts::Model {
    fn from(c: &Context) -> Self {
        let (mut notes_path, mut branch, mut host, mut number) = (None, None, None, None);
        match &c.key {
            ContextKey::Notes { path } => notes_path = Some(path.clone()),
            ContextKey::Branch { branch: b, .. } => branch = Some(b.clone()),
            ContextKey::PullRequest {
                host: h, number: n, ..
            } => {
                host = Some(h.clone());
                number = Some(*n);
            }
        }
        review_contexts::Model {
            id: c.id,
            kind: match c.key {
                ContextKey::Notes { .. } => ContextKind::Notes,
                ContextKey::Branch { .. } => ContextKind::Branch,
                ContextKey::PullRequest { .. } => ContextKind::PullRequest,
            },
            notes_path,
            repository: c.key.repository().map(str::to_owned),
            branch,
            host,
            number,
            revision: c.revision,
            last_refreshed_at: c.last_refreshed_at.clone(),
            refreshed_head: c.refreshed_head.clone(),
            refreshed_base: c.refreshed_base.clone(),
            refresh_status: c.refresh_status.clone(),
            refresh_error: c.refresh_error.clone(),
        }
    }
}
