use std::path::{Component, Path, PathBuf};

use serde::Serialize;
use specta::Type;

use crate::error::{Result, error};

/// The location of a local workspace on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    /// The directory that contains the workspace files.
    pub root: PathBuf,
}

impl Workspace {
    /// Builds the default workspace location without accessing the filesystem.
    pub fn from_home(home: PathBuf) -> Self {
        Self {
            root: home.join(".lgtm").join("workspace"),
        }
    }

    /// Builds a workspace for a chosen directory path without accessing the filesystem.
    pub fn from_path(root: PathBuf) -> Self {
        Self { root }
    }
}

/// The workspace location and its top-level entries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceListing {
    /// The absolute path to the workspace directory.
    pub root: String,
    /// The visible entries directly inside the workspace.
    pub entries: Vec<WorkspaceNode>,
}

/// A file or folder listed in the workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceNode {
    /// The path relative to the workspace root.
    pub path: String,
    /// The file or folder name, including any extension.
    pub name: String,
    /// The entry type: `directory`, `file`, or `unavailable`.
    pub kind: &'static str,
}

impl WorkspaceNode {
    // Public ------------------------------------------------------------------

    /// Trims a name and rejects names that cannot identify one workspace entry.
    pub fn validate_name(name: &str) -> Result<&str> {
        let name = name.trim();
        let mut components = Path::new(name).components();
        if name.is_empty()
            || name.contains(['/', '\\', ':', '\0'])
            || !matches!(components.next(), Some(Component::Normal(_)))
            || components.next().is_some()
            || name.starts_with(".lgtm-save-")
        {
            return Err(error(
                "invalidName",
                "Enter a name without slashes or colons.",
            ));
        }
        Ok(name)
    }

    /// Rejects relative paths that contain parent traversal, root prefixes, or drive separators.
    pub fn validate_relative_path(relative: &str) -> Result<&Path> {
        let path = Path::new(relative);
        if path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
            || relative.contains('\\')
            || relative.contains(':')
        {
            return Err(error(
                "outsideWorkspace",
                "Choose a file inside the workspace.",
            ));
        }
        Ok(path)
    }
}

// tests -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // tests -------------------------------------------------------------------

    mod given_workspace_domain_models {
        use super::*;

        mod when_validating_names_and_relative_paths {
            use super::*;

            #[test]
            fn should_trim_valid_names_and_reject_traversal_or_reserved_names() {
                // Arrange
                let valid = "  Notes.md  ";
                let invalid = ["", " ", ".", "..", "a/b", "a\\b", "a:b", ".lgtm-save-1"];

                // Act
                let trimmed = WorkspaceNode::validate_name(valid).unwrap();
                let invalid_codes: Vec<&str> = invalid
                    .into_iter()
                    .map(|candidate| WorkspaceNode::validate_name(candidate).unwrap_err().code)
                    .collect();
                let traversal = WorkspaceNode::validate_relative_path("../escape");

                // Assert
                assert_eq!(trimmed, "Notes.md");
                assert!(invalid_codes.into_iter().all(|code| code == "invalidName"));
                assert_eq!(traversal.unwrap_err().code, "outsideWorkspace");
            }
        }
    }
}
