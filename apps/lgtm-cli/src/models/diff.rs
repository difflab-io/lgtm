//! Diff hunk and diff line models for terminal and JSON review output.

use lgtm_forge::proto::lgtm::v1::{DiffHunkDetail, DiffLineDetail};
use serde_json::{Value, json};

use crate::models::common::format_diff_line_kind;

/// Single line within a diff hunk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffLine {
    pub line_number: u32,
    pub kind: &'static str,
    pub marker: char,
    pub content: String,
}

impl DiffLine {
    // Public ------------------------------------------------------------------

    /// Serialize this diff line to its JSON contract.
    pub fn to_json(&self) -> Value {
        json!({
            "kind": self.kind,
            "line_number": self.line_number,
            "content": self.content,
        })
    }
}

impl From<&DiffLineDetail> for DiffLine {
    fn from(line: &DiffLineDetail) -> Self {
        let (kind, marker) = format_diff_line_kind(line.kind.as_known());
        Self {
            line_number: line.line_number,
            kind,
            marker,
            content: line.content.clone(),
        }
    }
}

/// Diff hunk anchored to a file and line range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffHunk {
    pub file_path: String,
    pub commit_sha: String,
    pub start_line: u32,
    pub lines: Vec<DiffLine>,
}

impl DiffHunk {
    // Public ------------------------------------------------------------------

    /// Render the numbered diff lines for terminal display.
    pub fn render_lines(&self) -> String {
        self.lines
            .iter()
            .map(|line| {
                format!(
                    " {} {:>2} │ {}",
                    line.marker, line.line_number, line.content
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Serialize this diff hunk to its JSON contract.
    pub fn to_json(&self) -> Value {
        let lines: Vec<Value> = self.lines.iter().map(DiffLine::to_json).collect();
        json!({
            "file_path": self.file_path,
            "commit_sha": self.commit_sha,
            "start_line": self.start_line,
            "lines": lines,
        })
    }
}

impl From<&DiffHunkDetail> for DiffHunk {
    fn from(hunk: &DiffHunkDetail) -> Self {
        Self {
            file_path: hunk.file_path.clone(),
            commit_sha: hunk.commit_sha.clone(),
            start_line: hunk.start_line,
            lines: hunk.lines.iter().map(DiffLine::from).collect(),
        }
    }
}
