use super::{AnchorEvidence, AnchorPlacement, AnchorSource};
use crate::error::{Result, invalid};
use crate::{models as m, proto::lgtm::v1 as p};
use connectrpc::ConnectError;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// The original discussion location: general, whole file, or a line range.
/// Exact byte identity and bounded relocation text belong to AnchorEvidence, not this location.
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

    /// Resolve only against the caller's named, confirmed file association and displayed source.
    pub fn get_placement(
        &self,
        evidence: Option<&AnchorEvidence>,
        path: &str,
        displayed_source: Option<&AnchorSource>,
        content: Option<&str>,
        path_confirmed: bool,
    ) -> AnchorPlacement {
        if matches!(self, Self::General) {
            return AnchorPlacement::Unverified {
                reason: "Review-level feedback has no code location".into(),
            };
        }
        if !path_confirmed {
            return AnchorPlacement::Unverified {
                reason: "File association is not confirmed for this comparison".into(),
            };
        }
        let (displayed_source, content) = match (displayed_source, content) {
            (Some(source), Some(content)) => (source, content),
            (None, None) => {
                return AnchorPlacement::Orphaned {
                    reason: "File does not exist on the selected side".into(),
                };
            }
            _ => {
                return AnchorPlacement::Unverified {
                    reason: "Displayed source content is unavailable".into(),
                };
            }
        };
        if displayed_source.validate().is_err()
            || displayed_source.content_hash() != AnchorSource::hash(content.as_bytes())
        {
            return AnchorPlacement::Unverified {
                reason: "Displayed source identity does not match its bytes".into(),
            };
        }
        match self {
            Self::General => {
                return AnchorPlacement::Unverified {
                    reason: "Review-level feedback has no code location".into(),
                };
            }
            Self::File { .. } | Self::Line { .. } => {}
        }
        let Some(evidence) = evidence else {
            return AnchorPlacement::Unverified {
                reason: "Original source evidence is unavailable".into(),
            };
        };
        if evidence.validate().is_err() {
            return AnchorPlacement::Unverified {
                reason: "Original source evidence is invalid".into(),
            };
        }
        if matches!(evidence.source, AnchorSource::WorkingFile { .. })
            && evidence.source.content_hash() != displayed_source.content_hash()
        {
            return AnchorPlacement::Orphaned {
                reason: "Unstaged working source changed".into(),
            };
        }
        if let Self::File {
            path: original_path,
        } = self
        {
            let location = Self::File { path: path.into() };
            return if &evidence.source == displayed_source && original_path == path {
                AnchorPlacement::Exact {
                    location,
                    displayed_source: displayed_source.clone(),
                }
            } else {
                AnchorPlacement::Relocated {
                    location,
                    displayed_source: displayed_source.clone(),
                }
            };
        }
        let Self::Line {
            path: original_path,
            start,
            end,
            side,
            ..
        } = self
        else {
            unreachable!()
        };
        let at_original = evidence.source.content_hash() == displayed_source.content_hash()
            && AnchorEvidence::capture(displayed_source.clone(), content.as_bytes(), *start, *end)
                .is_ok_and(|actual| actual.selected_text == evidence.selected_text);
        let range = if at_original {
            Some((*start, *end))
        } else {
            evidence.get_matching_range(content)
        };
        let Some((start, end)) = range else {
            return AnchorPlacement::Outdated {
                reason: "Original selection was lost, changed, or became ambiguous".into(),
            };
        };
        let location = Self::Line {
            path: path.into(),
            start,
            end,
            side: side.clone(),
        };
        if at_original && original_path == path && evidence.source == *displayed_source {
            AnchorPlacement::Exact {
                location,
                displayed_source: displayed_source.clone(),
            }
        } else {
            AnchorPlacement::Relocated {
                location,
                displayed_source: displayed_source.clone(),
            }
        }
    }

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
