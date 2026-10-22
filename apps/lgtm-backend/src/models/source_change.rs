//! Confirmed source moves, deletions and restorations.
use crate::{models as m, proto::lgtm::v1 as p};
use connectrpc::ConnectError;
use serde::{Deserialize, Serialize};

/// Confirmed client observations, validated against the selected scope and saved evidence.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourceChange {
    /// A confirmed move of the selected saved-file scope or directory review root.
    LocationMoved {
        previous_path: String,
        current_path: String,
    },
    Moved {
        thread_id: uuid::Uuid,
        previous_path: String,
        current_path: String,
        source: super::AnchorSource,
    },
    Deleted {
        thread_id: uuid::Uuid,
        path: String,
    },
    WorkingSourceChanged {
        thread_id: uuid::Uuid,
        path: String,
        source: super::AnchorSource,
    },
    Restored {
        thread_id: uuid::Uuid,
        path: String,
        source: super::AnchorSource,
    },
}

impl SourceChange {
    // Public ------------------------------------------------------------------

    // RPC conversion ----------------------------------------------------------

    /// Validate and convert the RPC input to the domain model.
    pub(crate) fn from_proto(
        v: &p::SourceChange,
    ) -> std::result::Result<m::SourceChange, ConnectError> {
        match &v.kind {
            Some(p::source_change::Kind::LocationMoved(v)) => Ok(m::SourceChange::LocationMoved {
                previous_path: v.previous_path.clone(),
                current_path: v.current_path.clone(),
            }),
            Some(p::source_change::Kind::Moved(v)) => Ok(m::SourceChange::Moved {
                thread_id: uuid::Uuid::parse_str(&v.thread_id)
                    .map_err(|_| ConnectError::invalid_argument("Expected UUID"))?,
                previous_path: v.previous_path.clone(),
                current_path: v.current_path.clone(),
                source: m::AnchorSource::from_proto(&v.source)?,
            }),
            Some(p::source_change::Kind::Deleted(v)) => Ok(m::SourceChange::Deleted {
                thread_id: uuid::Uuid::parse_str(&v.thread_id)
                    .map_err(|_| ConnectError::invalid_argument("Expected UUID"))?,
                path: v.path.clone(),
            }),
            Some(p::source_change::Kind::WorkingSourceChanged(v)) => {
                Ok(m::SourceChange::WorkingSourceChanged {
                    thread_id: uuid::Uuid::parse_str(&v.thread_id)
                        .map_err(|_| ConnectError::invalid_argument("Expected UUID"))?,
                    path: v.path.clone(),
                    source: m::AnchorSource::from_proto(&v.source)?,
                })
            }
            Some(p::source_change::Kind::Restored(v)) => Ok(m::SourceChange::Restored {
                thread_id: uuid::Uuid::parse_str(&v.thread_id)
                    .map_err(|_| ConnectError::invalid_argument("Expected UUID"))?,
                path: v.path.clone(),
                source: m::AnchorSource::from_proto(&v.source)?,
            }),
            None => Err(ConnectError::invalid_argument(
                "Source change requires a typed observation",
            )),
        }
    }
}
