//! Domain models for inline unified diff hunks and lines.

use similar::{ChangeTag, TextDiff};

use crate::{
    models::CommentAnchor,
    proto::lgtm::v1::{DiffHunkDetail, DiffLineDetail, DiffLineKind as ProtoDiffLineKind},
};

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

impl DiffHunk {
    // Public ------------------------------------------------------------------

    /// Build inline unified diff hunks ($\pm 3$ context lines) in memory around each `CommentAnchor` for `ShowReview`.
    pub fn from_blobs(
        file_path: &str,
        commit_sha: &str,
        base_blob: Option<&str>,
        head_blob: Option<&str>,
        anchors: &[CommentAnchor],
    ) -> Vec<Self> {
        let old_text = base_blob.unwrap_or("");
        let new_text = head_blob.unwrap_or("");
        let diff = TextDiff::from_lines(old_text, new_text);

        let mut hunks = Vec::new();
        for anchor in anchors {
            if anchor.file_path != file_path {
                continue;
            }
            let window_start = anchor
                .start_line
                .unwrap_or(anchor.line)
                .saturating_sub(3)
                .max(1);
            let window_end = anchor.line.saturating_add(3);

            let mut lines = Vec::new();
            let mut new_line_cursor: u32 = 1;
            for change in diff.iter_all_changes() {
                let content = change
                    .value()
                    .trim_end_matches(&['\r', '\n'][..])
                    .to_owned();
                match change.tag() {
                    ChangeTag::Equal => {
                        if let Some(new_idx) = change.new_index() {
                            let line_num = u32::try_from(new_idx + 1).unwrap_or(1);
                            new_line_cursor = line_num.saturating_add(1);
                            if line_num >= window_start && line_num <= window_end {
                                lines.push(DiffLine {
                                    kind: DiffLineKind::Context,
                                    line_number: line_num,
                                    content,
                                });
                            }
                        }
                    }
                    ChangeTag::Insert => {
                        if let Some(new_idx) = change.new_index() {
                            let line_num = u32::try_from(new_idx + 1).unwrap_or(1);
                            new_line_cursor = line_num.saturating_add(1);
                            if line_num >= window_start && line_num <= window_end {
                                lines.push(DiffLine {
                                    kind: DiffLineKind::Added,
                                    line_number: line_num,
                                    content,
                                });
                            }
                        }
                    }
                    ChangeTag::Delete => {
                        if let Some(old_idx) = change.old_index() {
                            let line_num = u32::try_from(old_idx + 1).unwrap_or(1);
                            if new_line_cursor >= window_start && new_line_cursor <= window_end {
                                lines.push(DiffLine {
                                    kind: DiffLineKind::Removed,
                                    line_number: line_num,
                                    content,
                                });
                            }
                        }
                    }
                }
            }

            let start_line = lines
                .first()
                .map(|l| l.line_number)
                .unwrap_or(anchor.start_line.unwrap_or(anchor.line));
            hunks.push(Self {
                file_path: anchor.file_path.clone(),
                commit_sha: commit_sha.to_owned(),
                start_line,
                lines,
            });
        }

        hunks
    }
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

// tests -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_extract_unified_diff_hunk_around_comment_anchor_in_memory() {
        let base = "fn alpha() -> i32 {\n    1\n}\n";
        let head = "fn alpha() -> i32 {\n    10\n}\n";
        let anchor = CommentAnchor::new("src/lib.rs", None, 2, "deadbeef").unwrap();
        let hunks =
            DiffHunk::from_blobs("src/lib.rs", "deadbeef", Some(base), Some(head), &[anchor]);
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].file_path, "src/lib.rs");
        assert!(
            hunks[0].lines.iter().any(|l| l.kind == DiffLineKind::Added
                && l.line_number == 2
                && l.content == "    10")
        );
        assert!(
            hunks[0]
                .lines
                .iter()
                .any(|l| l.kind == DiffLineKind::Removed && l.content == "    1")
        );
    }
}
