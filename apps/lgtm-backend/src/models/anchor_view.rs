//! Placement against an editor's displayed source, including provider observations.
use super::{Anchor, AnchorPlacement, AnchorSource, ReviewComparison, SuppliedAnchor};
use crate::proto::lgtm::v1 as p;
use connectrpc::ConnectError;

/// Transient display inputs. These never replace a thread's immutable original evidence.
#[derive(Clone, Debug)]
pub struct AnchorView {
    pub path: String,
    pub logical_path: String,
    pub previous_path: String,
    pub side: String,
    pub confirmed_rename: bool,
    pub source: Option<AnchorSource>,
    pub content: Option<String>,
    pub comparison: Option<ReviewComparison>,
    pub historical: bool,
    pub unavailable: String,
}

impl AnchorView {
    // Public ------------------------------------------------------------------

    /// Return no annotation for another file or side. Unplaced feedback remains accessible.
    pub fn get_placement(
        &self,
        supplied: &SuppliedAnchor,
        notes: bool,
        has_new_source: bool,
    ) -> Option<AnchorPlacement> {
        let selected_side = match &supplied.original {
            Anchor::General => return None,
            Anchor::File { .. } => {
                if has_new_source {
                    "new"
                } else {
                    "old"
                }
            }
            Anchor::Line { side, .. } => supplied
                .forge_observation
                .as_ref()
                .filter(|o| !o.outdated)
                .and_then(|o| o.current.as_ref())
                .and_then(|a| match a {
                    Anchor::Line { side, .. } => Some(side.as_str()),
                    _ => None,
                })
                .unwrap_or(side),
        };
        if self.side != selected_side || !(notes || self.get_path_confirmation(supplied)) {
            return None;
        }
        if !self.unavailable.is_empty() {
            return Some(AnchorPlacement::Unverified {
                reason: self.unavailable.clone(),
            });
        }
        if let Some(source) = &self.source
            && (source.validate().is_err()
                || self.content.as_ref().is_none_or(|text| {
                    source.content_hash() != AnchorSource::hash(text.as_bytes())
                }))
        {
            return Some(AnchorPlacement::Unverified {
                reason: "Displayed source identity does not match its bytes".into(),
            });
        }
        if let Some(observed) = &supplied.forge_observation
            && self.comparison.as_ref().is_some_and(|comparison| matches!(comparison,
                ReviewComparison::PullRequest { base_commit, head_commit } if base_commit == &observed.base && head_commit == &observed.head))
            && !self.historical
        {
            if observed.outdated {
                return Some(AnchorPlacement::Outdated { reason: "Outdated on the current PR; original evidence is retained".into() });
            }
            if let (Some(current), Some(source)) = (&observed.current, &self.source) {
                let (path, side, range) = match current {
                    Anchor::File { path } => (path, None, None),
                    Anchor::Line { path, side, start, end } => (path, Some(side.as_str()), Some((*start, *end))),
                    Anchor::General => return None,
                };
                let matches = (path == &self.logical_path || path == &self.path)
                    && side.is_none_or(|side| side == self.side && matches!(source, AnchorSource::Commit { sha, .. } if sha == if side == "old" { &observed.base } else { &observed.head }));
                if matches {
                    let count = self.content.as_deref().unwrap_or("").lines().count() as i64;
                    return Some(if range.is_none_or(|(first, last)| first >= 1 && last >= first && last <= count) {
                        AnchorPlacement::Exact { location: current.clone(), displayed_source: source.clone() }
                    } else {
                        AnchorPlacement::Outdated { reason: "Provider location is unavailable in the displayed source".into() }
                    });
                }
            }
        }
        let result = supplied.original.get_placement(
            supplied.evidence.as_ref(),
            &self.path,
            self.source.as_ref(),
            self.content.as_deref(),
            true,
        );
        Some(
            if matches!(result, AnchorPlacement::Orphaned { .. })
                && supplied.evidence.as_ref().is_some_and(|e| {
                    matches!(e.source, AnchorSource::WorkingFile { .. })
                        && self
                            .source
                            .as_ref()
                            .is_some_and(|s| s.content_hash() != e.source.content_hash())
                })
            {
                AnchorPlacement::Stale {
                    reason: "Unstaged source changed; original evidence is retained".into(),
                }
            } else {
                result
            },
        )
    }

    pub(crate) fn from_proto(value: &p::AnchorView) -> Result<Self, ConnectError> {
        Anchor::File {
            path: value.path.clone(),
        }
        .validate()
        .map_err(ConnectError::from)?;
        if !["old", "new"].contains(&value.side.as_str()) {
            return Err(ConnectError::invalid_argument(
                "Displayed side must be old or new",
            ));
        }
        if value
            .content
            .as_ref()
            .is_some_and(|s| s.len() > 2 * 1024 * 1024)
        {
            return Err(ConnectError::invalid_argument(
                "Displayed source exceeds the text size limit",
            ));
        }
        Ok(Self {
            path: value.path.clone(),
            logical_path: value.logical_path.clone(),
            previous_path: value.previous_path.clone(),
            side: value.side.clone(),
            confirmed_rename: value.confirmed_rename,
            source: if value.source.is_set() {
                Some(AnchorSource::from_proto(&value.source)?)
            } else {
                None
            },
            content: value.content.clone(),
            comparison: ReviewComparison::from_proto(&value.comparison)?,
            historical: value.historical,
            unavailable: value.unavailable.clone(),
        })
    }

    // Private -----------------------------------------------------------------

    fn get_path_confirmation(&self, supplied: &SuppliedAnchor) -> bool {
        let path = match &supplied.original {
            Anchor::File { path } | Anchor::Line { path, .. } => path,
            Anchor::General => return false,
        };
        if !supplied.current_path.is_empty() {
            if self.logical_path == supplied.current_path
                || self.confirmed_rename && supplied.current_path == self.previous_path
            {
                return true;
            }
            return path == &self.path
                && matches!(
                    (supplied.evidence.as_ref().map(|e| &e.source), &self.source),
                    (Some(AnchorSource::Commit { sha: original, .. }), Some(AnchorSource::Commit { sha: displayed, .. })) if original == displayed
                );
        }
        path == &self.logical_path
            || path == &self.path
            || self.confirmed_rename && path == &self.previous_path
    }
}

// tests -----------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AnchorEvidence, ForgeObservation};

    // fixtures ----------------------------------------------------------------
    fn fixture() -> (AnchorView, SuppliedAnchor) {
        let content = "before\nselected\nafter\n";
        let source = AnchorSource::Commit {
            sha: "a".repeat(40),
            content_hash: AnchorSource::hash(content.as_bytes()),
        };
        let anchor = SuppliedAnchor {
            original: Anchor::Line {
                path: "file.md".into(),
                start: 2,
                end: 2,
                side: "new".into(),
            },
            evidence: Some(
                AnchorEvidence::capture(source.clone(), content.as_bytes(), 2, 2).unwrap(),
            ),
            current_path: String::new(),
            forge_observation: None,
        };
        let view = AnchorView {
            path: "file.md".into(),
            logical_path: "file.md".into(),
            previous_path: String::new(),
            side: "new".into(),
            confirmed_rename: false,
            source: Some(source),
            content: Some(content.into()),
            comparison: Some(ReviewComparison::PullRequest {
                base_commit: "b".repeat(40),
                head_commit: "a".repeat(40),
            }),
            historical: false,
            unavailable: String::new(),
        };
        (view, anchor)
    }

    // tests -------------------------------------------------------------------
    #[test]
    fn should_preserve_provider_status_while_verifying_historical_evidence() {
        let (mut view, mut anchor) = fixture();
        anchor.current_path = "renamed.md".into();
        anchor.forge_observation = Some(ForgeObservation {
            base: "b".repeat(40),
            head: "a".repeat(40),
            outdated: true,
            current: None,
        });
        let original = serde_json::to_value((
            &anchor.original,
            &anchor.evidence,
            &anchor.forge_observation,
        ))
        .unwrap();
        assert!(matches!(
            view.get_placement(&anchor, false, true),
            Some(AnchorPlacement::Outdated { .. })
        ));
        view.historical = true;
        assert!(matches!(
            view.get_placement(&anchor, false, true),
            Some(AnchorPlacement::Exact { .. })
        ));
        assert_eq!(
            serde_json::to_value((
                &anchor.original,
                &anchor.evidence,
                &anchor.forge_observation
            ))
            .unwrap(),
            original
        );
        anchor.current_path.clear();
        anchor.evidence = None;
        assert!(matches!(
            view.get_placement(&anchor, false, true),
            Some(AnchorPlacement::Unverified { .. })
        ));
        view.unavailable = "Source could not be read".into();
        assert!(
            matches!(view.get_placement(&anchor, false, true), Some(AnchorPlacement::Unverified { reason }) if reason == "Source could not be read")
        );
    }

    #[test]
    fn should_use_current_provider_coordinates_without_fabricating_original_evidence() {
        let (view, mut anchor) = fixture();
        anchor.evidence = None;
        anchor.forge_observation = Some(ForgeObservation {
            base: "b".repeat(40),
            head: "a".repeat(40),
            outdated: false,
            current: Some(Anchor::Line {
                path: "file.md".into(),
                start: 3,
                end: 3,
                side: "new".into(),
            }),
        });
        assert!(matches!(
            view.get_placement(&anchor, false, true),
            Some(AnchorPlacement::Exact {
                location: Anchor::Line { start: 3, .. },
                ..
            })
        ));
        let mut invalid = view;
        invalid.content = Some("different bytes".into());
        assert!(matches!(
            invalid.get_placement(&anchor, false, true),
            Some(AnchorPlacement::Unverified { .. })
        ));
    }

    #[test]
    fn should_report_changed_unstaged_sources_as_stale_without_guessing_a_line() {
        let (mut view, mut anchor) = fixture();
        let original = view.content.as_ref().unwrap();
        anchor.evidence = Some(
            AnchorEvidence::capture(
                AnchorSource::WorkingFile {
                    content_hash: AnchorSource::hash(original.as_bytes()),
                },
                original.as_bytes(),
                2,
                2,
            )
            .unwrap(),
        );
        view.content = Some("intro\nbefore\nselected\nafter\n".into());
        view.source = Some(AnchorSource::WorkingFile {
            content_hash: AnchorSource::hash(view.content.as_ref().unwrap().as_bytes()),
        });
        assert!(matches!(
            view.get_placement(&anchor, false, true),
            Some(AnchorPlacement::Stale { .. })
        ));
    }

    #[test]
    fn should_separate_unrelated_files_missing_sides_and_failed_reads() {
        let (mut view, anchor) = fixture();
        view.side = "old".into();
        assert!(view.get_placement(&anchor, false, true).is_none());
        view.side = "new".into();
        view.path = "other.md".into();
        view.logical_path = "other.md".into();
        assert!(view.get_placement(&anchor, false, true).is_none());
        view.path = "file.md".into();
        view.logical_path = "file.md".into();
        view.source = None;
        view.content = None;
        assert!(matches!(
            view.get_placement(&anchor, false, false),
            Some(AnchorPlacement::Orphaned { .. })
        ));
        view.unavailable = "Cannot read source".into();
        assert!(matches!(
            view.get_placement(&anchor, false, false),
            Some(AnchorPlacement::Unverified { .. })
        ));
    }
}
