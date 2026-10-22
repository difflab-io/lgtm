use super::{Anchor, AnchorSource};
use crate::{models as m, proto::lgtm::v1 as p};
use serde::{Deserialize, Serialize};

/// A derived result for one displayed source. Only confirmed variants carry code coordinates.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AnchorPlacement {
    Exact {
        location: Anchor,
        displayed_source: AnchorSource,
    },
    Relocated {
        location: Anchor,
        displayed_source: AnchorSource,
    },
    Outdated {
        reason: String,
    },
    Orphaned {
        reason: String,
    },
    Unverified {
        reason: String,
    },
}

impl AnchorPlacement {
    // Public ------------------------------------------------------------------

    // RPC conversion ----------------------------------------------------------

    /// Convert the domain value to its RPC representation.
    pub(crate) fn into_proto(self, index: usize) -> p::AnchorPlacement {
        use p::anchor_placement::Kind;
        let kind = match self {
            m::AnchorPlacement::Exact {
                location,
                displayed_source,
            } => Kind::Exact(Box::new(p::ConfirmedPlacement {
                location: Some(m::Anchor::into_proto(location)).into(),
                displayed_source: Some(m::AnchorSource::into_proto(displayed_source)).into(),
                ..Default::default()
            })),
            m::AnchorPlacement::Relocated {
                location,
                displayed_source,
            } => Kind::Relocated(Box::new(p::ConfirmedPlacement {
                location: Some(m::Anchor::into_proto(location)).into(),
                displayed_source: Some(m::AnchorSource::into_proto(displayed_source)).into(),
                ..Default::default()
            })),
            m::AnchorPlacement::Outdated { reason } => {
                Kind::Outdated(Box::new(p::UnplacedAnchor {
                    reason,
                    ..Default::default()
                }))
            }
            m::AnchorPlacement::Orphaned { reason } => {
                Kind::Orphaned(Box::new(p::UnplacedAnchor {
                    reason,
                    ..Default::default()
                }))
            }
            m::AnchorPlacement::Unverified { reason } => {
                Kind::Unverified(Box::new(p::UnplacedAnchor {
                    reason,
                    ..Default::default()
                }))
            }
        };
        p::AnchorPlacement {
            index: index as u32,
            kind: Some(kind),
            ..Default::default()
        }
    }
}
