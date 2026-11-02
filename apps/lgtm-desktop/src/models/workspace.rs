use std::path::{Component, Path, PathBuf};

use serde::Serialize;
use sha2::{Digest, Sha256};

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
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceListing {
    /// The absolute path to the workspace directory.
    pub root: String,
    /// The visible entries directly inside the workspace.
    pub entries: Vec<WorkspaceNode>,
}

/// A file or folder listed in the workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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

/// UTF-8 file contents and revision hash read from the workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileContent {
    /// The UTF-8 source text of the file.
    pub content: String,
    /// The SHA-256 hex digest of the file bytes.
    pub revision: String,
}

impl FileContent {
    /// Decodes UTF-8 bytes and computes a content revision digest.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
        let revision = Self::revision_for(&bytes);
        let content = String::from_utf8(bytes)
            .map_err(|_| error("unsupported", "Only UTF-8 text files can be opened."))?;
        Ok(Self { content, revision })
    }

    // Public ------------------------------------------------------------------

    /// Computes a stable SHA-256 hex digest for a byte slice.
    pub fn revision_for(bytes: &[u8]) -> String {
        let digest = Sha256::digest(bytes);
        let mut hex = String::with_capacity(digest.len() * 2);
        for byte in digest {
            use std::fmt::Write as _;
            let _ = write!(&mut hex, "{byte:02x}");
        }
        hex
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

        mod when_decoding_file_content_from_bytes {
            use super::*;

            #[test]
            fn should_decode_utf8_with_sha256_revision_and_reject_non_utf8_bytes() {
                // Arrange
                let valid_bytes = b"# Review\n".to_vec();
                let invalid_bytes = vec![0xff, 0xfe, 0xfd];

                // Act
                let decoded = FileContent::from_bytes(valid_bytes).unwrap();
                let rejected = FileContent::from_bytes(invalid_bytes);

                // Assert
                assert_eq!(decoded.content, "# Review\n");
                assert_eq!(decoded.revision.len(), 64);
                assert_eq!(rejected.unwrap_err().code, "unsupported");
            }
        }
    }
}
