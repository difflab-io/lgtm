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

    /// Builds a workspace for an existing directory on disk.
    pub fn from_path(root: PathBuf) -> Result<Self> {
        if !root.is_dir() {
            return Err(error("invalidWorkspace", "Choose an existing folder."));
        }
        Ok(Self { root })
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
