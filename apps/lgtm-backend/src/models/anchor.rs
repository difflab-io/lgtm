use crate::error::{Result, invalid};
use crate::{models as m, proto::lgtm::v1 as p};
use connectrpc::ConnectError;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// The original discussion location: general, whole file, or a line range.
/// Exact byte identity and bounded source excerpts belong to AnchorEvidence, not this location.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Anchor {
    #[default]
    General,
    File {
        path: String,
    },
    Line {
        path: String,
        start: i64,
        end: i64,
        side: String,
    },
}

impl Anchor {
    // Public ------------------------------------------------------------------

    pub fn validate(&self) -> Result<()> {
        let path = match self {
            Self::General => return Ok(()),
            Self::File { path } | Self::Line { path, .. } => path,
        };
        if path.is_empty()
            || Path::new(path).is_absolute()
            || Path::new(path)
                .components()
                .any(|p| matches!(p, std::path::Component::ParentDir))
        {
            return Err(invalid(
                "Anchor path must be a nonempty relative path without ..",
            ));
        }
        if let Self::Line {
            start, end, side, ..
        } = self
            && (*start < 1 || end < start || !["old", "new"].contains(&side.as_str()))
        {
            return Err(invalid(
                "Line anchor requires positive ordered lines and old/new side",
            ));
        }
        Ok(())
    }

    // RPC conversion ----------------------------------------------------------

    /// Convert the domain value to its RPC representation.
    pub(crate) fn into_proto(self) -> p::Anchor {
        let v = self;
        let mut out = p::Anchor::default();
        match v {
            m::Anchor::General => out.kind = "general".into(),
            m::Anchor::File { path } => {
                out.kind = "file".into();
                out.path = path;
            }
            m::Anchor::Line {
                path,
                start,
                end,
                side,
            } => {
                out.kind = "line".into();
                out.path = path;
                out.start = start;
                out.end = end;
                out.side = side;
            }
        }
        out
    }

    /// Validate and convert the RPC input to the domain model.
    pub(crate) fn from_proto(v: &p::Anchor) -> std::result::Result<m::Anchor, ConnectError> {
        if (v.kind != "line" && (v.start != 0 || v.end != 0 || !v.side.is_empty()))
            || ((v.kind.is_empty() || v.kind == "general") && !v.path.is_empty())
        {
            return Err(ConnectError::invalid_argument(
                "p::Anchor fields do not match its kind",
            ));
        }
        let out = match v.kind.as_str() {
            "" | "general" => m::Anchor::General,
            "file" => m::Anchor::File {
                path: v.path.clone(),
            },
            "line" => m::Anchor::Line {
                path: v.path.clone(),
                start: v.start,
                end: if v.end == 0 { v.start } else { v.end },
                side: if v.side.is_empty() {
                    "new".into()
                } else {
                    v.side.clone()
                },
            },
            _ => return Err(ConnectError::invalid_argument("Unknown anchor kind")),
        };
        out.validate().map_err(ConnectError::from)?;
        Ok(out)
    }
}
