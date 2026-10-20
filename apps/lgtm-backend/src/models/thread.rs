use super::super::database::models::threads;
use super::super::database::types::SourceKind;
use super::super::error::Result;
use super::super::error::invalid;
use super::{Actor, Anchor, AnchorEvidence, AnchorSource, ForgeObservation, OrphanReason};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Thread {
    pub id: uuid::Uuid,
    pub context_id: uuid::Uuid,
    pub origin_session_id: Option<uuid::Uuid>,
    pub author: Actor,
    pub anchor: Anchor,
    /// Original selected text for local content anchors; no full-file snapshot is stored.
    pub selected_text: Option<String>,
    /// Immutable source evidence. None means a legacy source has not been proven.
    pub evidence: Option<AnchorEvidence>,
    /// Confirmed current path; absent when no move has been observed.
    pub current_path: Option<String>,
    /// Original absolute source location for saved-file notes. It never follows a move.
    #[serde(default)]
    pub source_path: Option<String>,
    pub orphan_reason: Option<OrphanReason>,
    pub forge_observation: Option<ForgeObservation>,
    pub state: String,
    pub resolved_by: Option<Actor>,
    pub resolved_at: Option<String>,
    pub updated_by: Option<Actor>,
    pub updated_at: String,
    pub forge_thread_id: Option<String>,
    pub outdated: bool,
}

impl Thread {
    pub fn from_db_model(
        v: threads::Model,
        author: Actor,
        resolved_by: Option<Actor>,
        updated_by: Option<Actor>,
    ) -> Result<Self> {
        let required = |v: &Option<String>| {
            v.clone()
                .ok_or_else(|| invalid("Persisted anchor evidence is incomplete"))
        };
        let evidence = if v.source_kind == SourceKind::Legacy {
            // Earlier non-Git notes already validated and saved this exact byte hash and excerpt.
            // Legacy Git anchors have no equivalent proof and remain unknown.
            match (&v.reviewed_commit, &v.selected_text) {
                (Some(hash), Some(selected))
                    if hash.starts_with("sha256:") && v.anchor_kind == "line" =>
                {
                    let evidence = AnchorEvidence {
                        source: AnchorSource::LocalFile {
                            content_hash: hash.clone(),
                        },
                        selected_text: selected.clone(),
                        context_before: String::new(),
                        context_after: String::new(),
                    };
                    evidence.validate()?;
                    Some(evidence)
                }
                _ => None,
            }
        } else {
            let content_hash = required(&v.source_content_hash)?;
            let source = match v.source_kind {
                SourceKind::Commit => AnchorSource::Commit {
                    sha: required(&v.source_commit)?,
                    content_hash,
                },
                SourceKind::IndexFile => AnchorSource::IndexFile {
                    blob_oid: required(&v.source_blob_oid)?,
                    content_hash,
                },
                SourceKind::WorkingFile => AnchorSource::WorkingFile { content_hash },
                SourceKind::LocalFile => AnchorSource::LocalFile { content_hash },
                SourceKind::Legacy => unreachable!(),
            };
            let evidence = AnchorEvidence {
                source,
                selected_text: required(&v.selected_text)?,
                context_before: v.context_before.clone().unwrap_or_default(),
                context_after: v.context_after.clone().unwrap_or_default(),
            };
            evidence.validate()?;
            Some(evidence)
        };
        let forge_observation = v
            .observed_outdated
            .map(|outdated| -> Result<ForgeObservation> {
                let current = match (&v.observed_path, v.observed_start, v.observed_end) {
                    (Some(path), Some(start), Some(end)) => Some(Anchor::Line {
                        path: path.clone(),
                        start,
                        end,
                        side: required(&v.observed_side)?,
                        commit: required(&v.observed_commit)?,
                    }),
                    (Some(path), None, None) => Some(Anchor::File { path: path.clone() }),
                    (None, None, None) => None,
                    _ => return Err(invalid("Persisted forge location is incomplete")),
                };
                if let Some(anchor) = &current {
                    anchor.validate()?;
                }
                Ok(ForgeObservation {
                    base: required(&v.observed_base)?,
                    head: required(&v.observed_head)?,
                    outdated,
                    current,
                })
            })
            .transpose()?;
        Ok(Thread {
            evidence,
            current_path: v.current_path,
            source_path: v.source_path,
            orphan_reason: v.orphan_reason,
            forge_observation,
            id: v.id,
            context_id: v.context_id,
            origin_session_id: v.origin_session_id,
            state: v.state,
            resolved_at: v.resolved_at,
            updated_at: v.updated_at,
            forge_thread_id: v.forge_thread_id,
            outdated: v.outdated,
            selected_text: v.selected_text,
            author,
            resolved_by,
            updated_by,
            anchor: match v.anchor_kind.as_str() {
                "file" => Anchor::File {
                    path: v.file_path.unwrap_or_default(),
                },
                "line" => Anchor::Line {
                    path: v.file_path.unwrap_or_default(),
                    start: v.start_line.unwrap_or_default(),
                    end: v.end_line.unwrap_or_default(),
                    side: v.side.unwrap_or_default(),
                    commit: v.reviewed_commit.unwrap_or_default(),
                },
                _ => Anchor::General,
            },
        })
    }
}

impl From<&Thread> for threads::Model {
    fn from(v: &Thread) -> Self {
        let mut row = Self {
            id: v.id,
            context_id: v.context_id,
            origin_session_id: v.origin_session_id,
            author_id: v.author.id,
            anchor_kind: match v.anchor {
                Anchor::General => "general",
                Anchor::File { .. } => "file",
                Anchor::Line { .. } => "line",
            }
            .into(),
            file_path: match &v.anchor {
                Anchor::General => None,
                Anchor::File { path } | Anchor::Line { path, .. } => Some(path.clone()),
            },
            side: match &v.anchor {
                Anchor::Line { side, .. } => Some(side.clone()),
                _ => None,
            },
            source_kind: SourceKind::Legacy,
            source_blob_oid: None,
            source_commit: None,
            source_content_hash: None,
            context_before: None,
            context_after: None,
            current_path: v.current_path.clone(),
            source_path: v.source_path.clone(),
            orphan_reason: v.orphan_reason,
            observed_base: None,
            observed_head: None,
            observed_outdated: None,
            observed_path: None,
            observed_side: None,
            observed_start: None,
            observed_end: None,
            observed_commit: None,
            selected_text: v.selected_text.clone(),
            reviewed_commit: match &v.anchor {
                Anchor::Line { commit, .. } => Some(commit.clone()),
                _ => None,
            },
            start_line: match &v.anchor {
                Anchor::Line { start, .. } => Some(*start),
                _ => None,
            },
            end_line: match &v.anchor {
                Anchor::Line { end, .. } => Some(*end),
                _ => None,
            },
            state: v.state.clone(),
            resolved_by_id: v.resolved_by.as_ref().map(|a| a.id),
            resolved_at: v.resolved_at.clone(),
            updated_by_id: v.updated_by.as_ref().map(|a| a.id),
            updated_at: v.updated_at.clone(),
            forge_thread_id: v.forge_thread_id.clone(),
            outdated: v.outdated,
        };
        if let Some(evidence) = &v.evidence {
            row.source_kind = match &evidence.source {
                AnchorSource::Commit { sha, .. } => {
                    row.source_commit = Some(sha.clone());
                    SourceKind::Commit
                }
                AnchorSource::IndexFile { blob_oid, .. } => {
                    row.source_blob_oid = Some(blob_oid.clone());
                    SourceKind::IndexFile
                }
                AnchorSource::WorkingFile { .. } => SourceKind::WorkingFile,
                AnchorSource::LocalFile { .. } => SourceKind::LocalFile,
            };
            row.source_content_hash = Some(evidence.source.content_hash().into());
            row.selected_text = Some(evidence.selected_text.clone());
            row.context_before = Some(evidence.context_before.clone());
            row.context_after = Some(evidence.context_after.clone());
        }
        if let Some(observation) = &v.forge_observation {
            row.observed_base = Some(observation.base.clone());
            row.observed_head = Some(observation.head.clone());
            row.observed_outdated = Some(observation.outdated);
            match &observation.current {
                Some(Anchor::Line {
                    path,
                    start,
                    end,
                    side,
                    commit,
                }) => {
                    row.observed_path = Some(path.clone());
                    row.observed_start = Some(*start);
                    row.observed_end = Some(*end);
                    row.observed_side = Some(side.clone());
                    row.observed_commit = Some(commit.clone());
                }
                Some(Anchor::File { path }) => row.observed_path = Some(path.clone()),
                _ => {}
            }
        }
        row
    }
}
