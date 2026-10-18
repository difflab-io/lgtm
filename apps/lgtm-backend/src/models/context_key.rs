use super::super::database::repositories;
use crate::error::{Result, invalid};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ContextKey {
    Notes {
        path: String,
    },
    Branch {
        repository: String,
        branch: String,
    },
    PullRequest {
        host: String,
        repository: String,
        number: i64,
    },
}

impl ContextKey {
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Notes { path } if !Path::new(path).is_absolute() => {
                return Err(invalid("Notes require a canonical absolute path"));
            }
            Self::Branch { repository, branch }
                if repository.is_empty()
                    || !branch.starts_with("refs/heads/")
                    || branch.len() <= 11 =>
            {
                return Err(invalid(
                    "Branch context requires repository and full refs/heads/ name",
                ));
            }
            Self::PullRequest {
                host,
                repository,
                number,
            } if host.is_empty() || repository.is_empty() || *number < 1 => {
                return Err(invalid(
                    "PR context requires host, repository and positive number",
                ));
            }
            _ => {}
        }
        Ok(())
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Self::Notes { .. } => "notes",
            Self::Branch { .. } => "branch",
            Self::PullRequest { .. } => "pr",
        }
    }

    pub fn repository(&self) -> Option<&str> {
        match self {
            Self::Notes { .. } => None,
            Self::Branch { repository, .. } | Self::PullRequest { repository, .. } => {
                Some(repository)
            }
        }
    }

    pub fn label(&self) -> String {
        match self {
            Self::Notes { path } => format!("notes:{path}"),
            Self::Branch { repository, branch } => {
                format!("{repository}@{}", branch.trim_start_matches("refs/heads/"))
            }
            Self::PullRequest {
                host,
                repository,
                number,
            } => format!(
                "{}#{number}",
                if repository.starts_with(&format!("{host}/")) {
                    repository.clone()
                } else {
                    format!("{host}/{repository}")
                }
            ),
        }
    }

    pub fn as_database_key(&self) -> repositories::review_context::ContextKey<'_> {
        match self {
            ContextKey::Notes { path } => repositories::review_context::ContextKey::Notes(path),
            ContextKey::Branch { repository, branch } => {
                repositories::review_context::ContextKey::Branch { repository, branch }
            }
            ContextKey::PullRequest {
                host,
                repository,
                number,
            } => repositories::review_context::ContextKey::PullRequest {
                host,
                repository,
                number: *number,
            },
        }
    }
}
