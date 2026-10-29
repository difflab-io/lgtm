//! A client-supplied anchor and its confirmed current path for one displayed diff.
use super::{Anchor, AnchorEvidence, AnchorPlacement, DiffContent, ForgeObservation};
use crate::proto::lgtm::v1 as p;
use connectrpc::ConnectError;
use std::path::Path;

/// Transient placement input. It never changes the persisted original anchor.
pub struct SuppliedAnchor {
    pub original: Anchor,
    pub evidence: Option<AnchorEvidence>,
    pub current_path: String,
    pub forge_observation: Option<ForgeObservation>,
}

impl SuppliedAnchor {
    // Public ------------------------------------------------------------------

    /// Validate the saved anchor and source evidence supplied by an API client.
    pub(crate) fn from_proto(value: &p::SuppliedAnchor) -> Result<Self, ConnectError> {
        Ok(Self {
            original: Anchor::from_proto(&value.original)?,
            evidence: AnchorEvidence::from_proto_anchor(&value.original)?,
            current_path: value.current_path.clone(),
            forge_observation: if value.forge_observation.is_set() {
                Some(ForgeObservation::from_proto(&value.forge_observation)?)
            } else {
                None
            },
        })
    }

    /// Derive placement only from confirmed paths, matching source identity or bounded text evidence.
    pub(crate) fn get_placement(
        self,
        content: &DiffContent,
        directory_root: Option<&Path>,
    ) -> Result<AnchorPlacement, ConnectError> {
        if !self.current_path.is_empty() {
            let current = Path::new(&self.current_path);
            if current.is_absolute() && directory_root.is_some() {
                if current
                    .components()
                    .any(|part| matches!(part, std::path::Component::ParentDir))
                {
                    return Err(ConnectError::invalid_argument(
                        "Current source location cannot contain ..",
                    ));
                }
            } else {
                Anchor::File {
                    path: self.current_path.clone(),
                }
                .validate()
                .map_err(ConnectError::from)?;
            }
        }
        let side = match &self.original {
            Anchor::Line { side, .. } => side.as_str(),
            _ => {
                if content.new.is_some() {
                    "new"
                } else {
                    "old"
                }
            }
        };
        let displayed = if side == "old" {
            &content.old
        } else {
            &content.new
        };
        let view = super::AnchorView {
            path: displayed
                .as_ref()
                .map(|s| &s.path)
                .unwrap_or(&content.file.path)
                .clone(),
            logical_path: directory_root
                .map(|root| root.join(&content.file.path).to_string_lossy().into_owned())
                .unwrap_or_else(|| content.file.path.clone()),
            previous_path: content.file.previous_path.clone(),
            side: side.into(),
            confirmed_rename: !content.file.previous_path.is_empty()
                && (content.file.status == "renamed" || content.file.status.starts_with('R')),
            source: displayed.as_ref().map(|s| s.source.clone()),
            content: displayed.as_ref().map(|s| s.content.clone()),
            comparison: None,
            historical: false,
            unavailable: String::new(),
        };
        Ok(view
            .get_placement(&self, false, content.new.is_some())
            .unwrap_or(AnchorPlacement::Unverified {
                reason: "File association is not confirmed for this comparison".into(),
            }))
    }
}
