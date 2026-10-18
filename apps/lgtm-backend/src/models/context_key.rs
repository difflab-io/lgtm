use super::super::database::repositories::review_context as db;
use super::{ForgeRepositoryUrl, LocalLocation};
use crate::error::{Result, invalid};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ContextKey {
    Local {
        location: LocalLocation,
    },
    ForgeBacked {
        repository_url: ForgeRepositoryUrl,
        number: i64,
    },
}

impl ContextKey {
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Local { location } => location.validate(),
            Self::ForgeBacked { number, .. } if *number < 1 => Err(invalid(
                "Forge-backed reviews require a positive PR/MR number",
            )),
            Self::ForgeBacked { .. } => Ok(()),
        }
    }

    pub fn is_forge_backed(&self) -> bool {
        matches!(self, Self::ForgeBacked { .. })
    }

    pub fn is_notes(&self) -> bool {
        matches!(
            self,
            Self::Local {
                location: LocalLocation::Notes { .. }
            }
        )
    }

    pub fn repository_url(&self) -> Option<&ForgeRepositoryUrl> {
        match self {
            Self::ForgeBacked { repository_url, .. } => Some(repository_url),
            Self::Local { .. } => None,
        }
    }

    pub fn git_dir(&self) -> Option<&str> {
        match self {
            Self::Local {
                location: LocalLocation::Branch { git_dir, .. },
            } => Some(git_dir),
            _ => None,
        }
    }

    pub fn label(&self) -> String {
        match self {
            Self::Local {
                location: LocalLocation::Notes { path },
            } => format!("notes:{path}"),
            Self::Local {
                location: LocalLocation::Branch { git_dir, branch },
            } => format!("{git_dir}@{}", branch.trim_start_matches("refs/heads/")),
            Self::ForgeBacked {
                repository_url,
                number,
            } => format!("{}#{number}", repository_url.as_str()),
        }
    }

    pub fn as_database_key(&self) -> db::ContextKey<'_> {
        match self {
            Self::Local {
                location: LocalLocation::Notes { path },
            } => db::ContextKey::Local(db::LocalLocation::Notes(path)),
            Self::Local {
                location: LocalLocation::Branch { git_dir, branch },
            } => db::ContextKey::Local(db::LocalLocation::Branch { git_dir, branch }),
            Self::ForgeBacked {
                repository_url,
                number,
            } => db::ContextKey::ForgeBacked {
                repository_url: repository_url.as_str(),
                number: *number,
            },
        }
    }
}

// tests -----------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_keep_local_and_forge_backed_identity_shapes_distinct() {
        let local = ContextKey::Local {
            location: LocalLocation::Notes {
                path: "/notes".into(),
            },
        };
        let value = serde_json::to_value(&local).unwrap();
        assert_eq!(value["kind"], "local");
        assert_eq!(serde_json::from_value::<ContextKey>(value).unwrap(), local);
        for value in [
            serde_json::json!({"kind":"local","location":{"kind":"notes","path":"/notes"},"repository_url":"https://github.com/acme/repo"}),
            serde_json::json!({"kind":"forge_backed","repository_url":"local:/repo","number":1}),
            serde_json::json!({"kind":"forge_backed","number":1}),
        ] {
            assert!(serde_json::from_value::<ContextKey>(value).is_err());
        }
        let invalid = ContextKey::ForgeBacked {
            repository_url: ForgeRepositoryUrl::parse("https://github.com/acme/repo").unwrap(),
            number: 0,
        };
        assert!(invalid.validate().is_err());
    }
}
