//! Domain model for UTF-8 file contents, SHA-256 content revision digests, and LSP status enrichment.

use serde::Serialize;
use sha2::{Digest, Sha256};
use specta::Type;

use crate::error::{Result, error};

/// Tauri IPC response payload returned by `workspace_read` and `workspace_write`, bundling the decoded UTF-8 document text, its SHA-256 content revision fingerprint for change tracking, and the initial LSP language and server status synchronized when the file is opened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FileContent {
    /// The UTF-8 source text of the file.
    pub content: String,
    /// The lowercase SHA-256 hex digest of the on-disk file bytes used to detect content changes across reads and writes.
    pub revision: String,
    /// Optional detected language identifier for the file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language_id: Option<String>,
    /// Optional active LSP server status for the file's language and project root.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lsp_status: Option<lgtm_lsp::LspServerStatusEvent>,
}

impl FileContent {
    /// Decodes UTF-8 bytes and computes a content revision digest.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
        let revision = Self::revision_for(&bytes);
        let content = String::from_utf8(bytes)
            .map_err(|_| error("unsupported", "Only UTF-8 text files can be opened."))?;
        Ok(Self {
            content,
            revision,
            language_id: None,
            lsp_status: None,
        })
    }

    // Public ------------------------------------------------------------------

    /// Attaches detected language and LSP server status metadata to the file content.
    pub fn with_lsp(
        mut self,
        language_id: Option<String>,
        lsp_status: Option<lgtm_lsp::LspServerStatusEvent>,
    ) -> Self {
        self.language_id = language_id;
        self.lsp_status = lsp_status;
        self
    }

    /// Computes the lowercase SHA-256 hex digest used as a deterministic content revision fingerprint across file reads and writes.
    pub fn revision_for(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }
}

// tests -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // tests -------------------------------------------------------------------

    mod given_raw_file_bytes {
        use super::*;

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
                assert_eq!(
                    decoded.revision,
                    "60390e262951c11c87fb37a0bba58ec02f73889fe444964faf0037e3adce528a"
                );
                assert_eq!(rejected.unwrap_err().code, "unsupported");
            }
        }

        mod when_computing_content_revision {
            use super::*;

            #[test]
            fn should_return_deterministic_sha256_hex_and_change_when_bytes_differ() {
                // Arrange
                let first = b"fn main() {}\n";
                let second = b"fn main() { println!(\"hi\"); }\n";

                // Act
                let rev_a1 = FileContent::revision_for(first);
                let rev_a2 = FileContent::revision_for(first);
                let rev_b = FileContent::revision_for(second);

                // Assert
                assert_eq!(rev_a1.len(), 64);
                assert_eq!(rev_a1, rev_a2);
                assert_ne!(rev_a1, rev_b);
            }
        }

        mod when_attaching_lsp_metadata {
            use super::*;

            #[test]
            fn should_populate_language_id_and_server_status() {
                // Arrange
                let base = FileContent::from_bytes(b"pub fn add() {}\n".to_vec()).unwrap();
                let status = lgtm_lsp::LspServerStatusEvent {
                    workspace_root: "/tmp/demo".to_string(),
                    language_id: "rust".to_string(),
                    project_root: "/tmp/demo".to_string(),
                    command: "rust-analyzer".to_string(),
                    status: lgtm_lsp::LspServerStatus::Ready,
                    message: None,
                    error_count: 0,
                    warning_count: 0,
                };

                // Act
                let enriched = base.with_lsp(Some("rust".to_string()), Some(status.clone()));

                // Assert
                assert_eq!(enriched.language_id.as_deref(), Some("rust"));
                assert_eq!(enriched.lsp_status, Some(status));
            }
        }
    }
}
