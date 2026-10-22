//! Atomic note-copy input with source attribution.
use super::MutationGuard;

/// Selected root notes and confirmed destination anchors. Replies remain at the source.
#[derive(Clone, Debug)]
pub struct NotePromotion {
    pub comment_ids: Vec<uuid::Uuid>,
    pub anchors: Vec<super::Anchor>,
    pub evidence: Vec<Option<super::AnchorEvidence>>,
    pub expected_source_version: Option<i64>,
    pub guard: MutationGuard,
}
