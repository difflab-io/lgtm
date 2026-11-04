use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::error::{Result, error};

/// UTF-8 file contents, revision hash, and optional LSP metadata read from the workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileContent {
    /// The UTF-8 source text of the file.
    pub content: String,
    /// The SHA-256 hex digest of the file bytes.
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
                assert_eq!(decoded.revision.len(), 64);
                assert_eq!(rejected.unwrap_err().code, "unsupported");
            }
        }
    }
}
