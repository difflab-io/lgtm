//! Transient diff descriptors and selected-source data. No review ownership or persistence.
use super::AnchorSource;
use crate::{models as m, proto::lgtm::v1 as p};

#[derive(Clone, Debug)]
pub struct ContentSide {
    pub path: String,
    pub source: AnchorSource,
    pub content: String,
}

impl ContentSide {
    // Public ------------------------------------------------------------------

    // RPC conversion ----------------------------------------------------------

    /// Convert the domain value to its RPC representation.
    pub(crate) fn into_proto(self) -> p::ContentSide {
        let value = self;
        p::ContentSide {
            path: value.path,
            source: Some(m::AnchorSource::into_proto(value.source)).into(),
            content: value.content,
            ..Default::default()
        }
    }
}
