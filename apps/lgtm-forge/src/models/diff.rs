//! Domain models for inline unified diff hunks and lines.

use crate::proto::lgtm::v1::{DiffHunkDetail, DiffLineDetail, DiffLineKind as ProtoDiffLineKind};

/// Change classification of a single unified diff line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffLineKind {
    Context,
    Added,
    Removed,
}

/// Single line within a unified diff hunk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub line_number: u32,
    pub content: String,
}

/// Contiguous block of unified diff lines extracted around a comment anchor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffHunk {
    pub file_path: String,
    pub commit_sha: String,
    pub start_line: u32,
    pub lines: Vec<DiffLine>,
}

impl From<DiffLineKind> for ProtoDiffLineKind {
    fn from(kind: DiffLineKind) -> Self {
        match kind {
            DiffLineKind::Context => Self::DIFF_LINE_KIND_CONTEXT,
            DiffLineKind::Added => Self::DIFF_LINE_KIND_ADDED,
            DiffLineKind::Removed => Self::DIFF_LINE_KIND_REMOVED,
        }
    }
}

impl From<DiffLine> for DiffLineDetail {
    fn from(line: DiffLine) -> Self {
        Self {
            kind: ProtoDiffLineKind::from(line.kind).into(),
            line_number: line.line_number,
            content: line.content,
            ..Default::default()
        }
    }
}

impl From<DiffHunk> for DiffHunkDetail {
    fn from(hunk: DiffHunk) -> Self {
        Self {
            file_path: hunk.file_path,
            commit_sha: hunk.commit_sha,
            start_line: hunk.start_line,
            lines: hunk.lines.into_iter().map(DiffLineDetail::from).collect(),
            ..Default::default()
        }
    }
}
