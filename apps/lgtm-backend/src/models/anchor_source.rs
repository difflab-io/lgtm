use crate::error::{Result, invalid};
use crate::{models as m, proto::lgtm::v1 as p};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Immutable identity of the saved bytes selected by a reviewer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AnchorSource {
    Commit {
        sha: String,
        content_hash: String,
    },
    IndexFile {
        blob_oid: String,
        content_hash: String,
    },
    WorkingFile {
        content_hash: String,
    },
    LocalFile {
        content_hash: String,
    },
}

impl AnchorSource {
    // Public ------------------------------------------------------------------

    pub fn content_hash(&self) -> &str {
        match self {
            Self::Commit { content_hash, .. }
            | Self::IndexFile { content_hash, .. }
            | Self::WorkingFile { content_hash }
            | Self::LocalFile { content_hash } => content_hash,
        }
    }

    pub fn validate(&self) -> Result<()> {
        let hash = self.content_hash();
        if !hash
            .strip_prefix("sha256:")
            .is_some_and(|value| value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err(invalid(
                "Source requires a SHA-256 identity of the exact saved bytes",
            ));
        }
        let oid = match self {
            Self::Commit { sha, .. } => Some(sha),
            Self::IndexFile { blob_oid, .. } => Some(blob_oid),
            _ => None,
        };
        if oid.is_some_and(|value| {
            ![40, 64].contains(&value.len()) || !value.bytes().all(|b| b.is_ascii_hexdigit())
        }) {
            return Err(invalid("Git source requires a full object identity"));
        }
        Ok(())
    }

    pub fn hash(bytes: &[u8]) -> String {
        format!("sha256:{:x}", Sha256::digest(bytes))
    }

    // RPC conversion ----------------------------------------------------------

    /// Convert the domain value to its RPC representation.
    pub(crate) fn into_proto(self) -> p::AnchorSource {
        let v = self;
        p::AnchorSource {
            kind: Some(match v {
                m::AnchorSource::Commit { sha, content_hash } => p::CommitSource {
                    sha,
                    content_hash,
                    ..Default::default()
                }
                .into(),
                m::AnchorSource::IndexFile {
                    blob_oid,
                    content_hash,
                } => p::IndexFileSource {
                    blob_oid,
                    content_hash,
                    ..Default::default()
                }
                .into(),
                m::AnchorSource::WorkingFile { content_hash } => p::WorkingFileSource {
                    content_hash,
                    ..Default::default()
                }
                .into(),
                m::AnchorSource::LocalFile { content_hash } => p::LocalFileSource {
                    content_hash,
                    ..Default::default()
                }
                .into(),
            }),
            ..Default::default()
        }
    }
}
