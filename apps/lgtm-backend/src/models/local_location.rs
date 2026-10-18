use crate::error::{Result, invalid};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Local notes or a branch in a local Git repository, with no forge requirement.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum LocalLocation {
    Notes { path: String },
    Branch { git_dir: String, branch: String },
}

impl LocalLocation {
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Notes { path } if !Path::new(path).is_absolute() => {
                Err(invalid("Notes require a canonical absolute path"))
            }
            Self::Branch { git_dir, branch }
                if !Path::new(git_dir).is_absolute()
                    || !branch.starts_with("refs/heads/")
                    || branch.len() <= 11 =>
            {
                Err(invalid(
                    "Local branch requires an absolute Git directory and full refs/heads/ branch name",
                ))
            }
            _ => Ok(()),
        }
    }
}

// tests -----------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_require_absolute_local_locations_and_full_branch_names() {
        assert!(
            LocalLocation::Notes {
                path: "/notes".into()
            }
            .validate()
            .is_ok()
        );
        assert!(
            LocalLocation::Notes {
                path: "relative".into()
            }
            .validate()
            .is_err()
        );
        assert!(
            LocalLocation::Branch {
                git_dir: "/repo/.git".into(),
                branch: "refs/heads/main".into()
            }
            .validate()
            .is_ok()
        );
        assert!(
            LocalLocation::Branch {
                git_dir: "local:repo".into(),
                branch: "main".into()
            }
            .validate()
            .is_err()
        );
    }
}
