//! A client-supplied anchor and its confirmed current path for one displayed diff.
use super::{Anchor, AnchorEvidence, AnchorPlacement, DiffContent};
use crate::proto::lgtm::v1 as p;
use connectrpc::ConnectError;
use std::path::Path;

/// Transient placement input. It never changes the persisted original anchor.
pub struct SuppliedAnchor {
    pub original: Anchor,
    pub evidence: Option<AnchorEvidence>,
    pub current_path: String,
}

impl SuppliedAnchor {
    // Public ------------------------------------------------------------------

    /// Validate the saved anchor and source evidence supplied by an API client.
    pub(crate) fn from_proto(value: &p::SuppliedAnchor) -> Result<Self, ConnectError> {
        Ok(Self {
            original: Anchor::from_proto(&value.original)?,
            evidence: AnchorEvidence::from_proto_anchor(&value.original)?,
            current_path: value.current_path.clone(),
        })
    }

    /// Derive placement only from confirmed paths, matching source identity or bounded text evidence.
    pub(crate) fn get_placement(
        self,
        content: &DiffContent,
        directory_root: Option<&Path>,
    ) -> Result<AnchorPlacement, ConnectError> {
        let Self {
            original,
            evidence,
            current_path,
        } = self;
        if !(current_path.is_empty()
            || directory_root.is_some() && std::path::Path::new(&current_path).is_absolute())
        {
            super::Anchor::File {
                path: current_path.clone(),
            }
            .validate()
            .map_err(ConnectError::from)?;
        }
        let original_path = match &original {
            super::Anchor::File { path } | super::Anchor::Line { path, .. } => path.as_str(),
            super::Anchor::General => "",
        };
        let displayed = match &original {
            super::Anchor::Line { side, .. } if side == "old" => content.old.as_ref(),
            super::Anchor::File { .. } => content.new.as_ref().or(content.old.as_ref()),
            _ => content.new.as_ref(),
        };
        let path = displayed
            .map(|s| s.path.as_str())
            .unwrap_or(content.file.path.as_str());
        let renamed = !content.file.previous_path.is_empty()
            && (content.file.status == "renamed" || content.file.status.starts_with('R'));
        let current_matches = if let Some(root) = directory_root
            .as_ref()
            .filter(|_| std::path::Path::new(&current_path).is_absolute())
        {
            if std::path::Path::new(&current_path)
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
            {
                return Err(ConnectError::invalid_argument(
                    "Current source location cannot contain ..",
                ));
            }
            std::path::Path::new(&current_path) == root.join(&content.file.path)
        } else {
            current_path == content.file.path
                || renamed && current_path == content.file.previous_path
        };
        // A current association wins over a reused old filename. An exact historical commit remains independently verifiable.
        let historical = evidence.as_ref().is_some_and(|evidence| {
                matches!((&evidence.source, displayed.map(|side| &side.source)),
                    (super::AnchorSource::Commit { sha: origin, .. }, Some(super::AnchorSource::Commit { sha: shown, .. })) if origin == shown && original_path == path)
            });
        let confirmed = if current_path.is_empty() {
            original_path == content.file.path
                || renamed && original_path == content.file.previous_path
        } else {
            current_matches || historical
        };
        let result = original.get_placement(
            evidence.as_ref(),
            path,
            displayed.map(|s| &s.source),
            displayed.map(|s| s.content.as_str()),
            confirmed,
        );
        Ok(result)
    }
}
