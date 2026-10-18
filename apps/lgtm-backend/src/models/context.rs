use super::super::{
    database::models::review_contexts,
    error::{Result, invalid},
};
use super::{ContextKey, ContextKind, ForgeRepositoryUrl, LocalLocation};
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
        let missing = || invalid("Invalid persisted context shape");
        let key = match row.kind {
            ContextKind::Local => ContextKey::Local {
                location: if let Some(path) = row.notes_path {
                    LocalLocation::Notes { path }
                } else {
                    LocalLocation::Branch {
                        git_dir: row.git_dir.ok_or_else(missing)?,
                        branch: row.branch.ok_or_else(missing)?,
                    }
                },
            },
            ContextKind::ForgeBacked => ContextKey::ForgeBacked {
                repository_url: ForgeRepositoryUrl::parse(
                    &row.repository_url.ok_or_else(missing)?,
                )?,
                number: row.number.ok_or_else(missing)?,
            },
        };
        key.validate()?;
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
        let (notes_path, git_dir, branch, repository_url, number) = match &c.key {
            ContextKey::Local {
                location: LocalLocation::Notes { path },
            } => (Some(path.clone()), None, None, None, None),
            ContextKey::Local {
                location: LocalLocation::Branch { git_dir, branch },
            } => (
                None,
                Some(git_dir.clone()),
                Some(branch.clone()),
                None,
                None,
            ),
            ContextKey::ForgeBacked {
                repository_url,
                number,
            } => (
                None,
                None,
                None,
                Some(repository_url.as_str().to_owned()),
                Some(*number),
            ),
        };
        review_contexts::Model {
            id: c.id,
            kind: if c.key.is_forge_backed() {
                ContextKind::ForgeBacked
            } else {
                ContextKind::Local
            },
            notes_path,
            git_dir,
            branch,
            repository_url,
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
