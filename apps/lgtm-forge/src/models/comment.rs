//! Domain model for inline diff comments, thread replies, and line anchors.

use std::collections::BTreeMap;

use similar::{ChangeTag, TextDiff};
use uuid::Uuid;

use crate::{
    database::{self, CommentAnchorShift},
    error::{Error, Result},
    models::Identity,
    proto::lgtm::v1::{CommentAnchorDetail, CommentDetail, IdentitySummary},
};

/// Unified diff coordinate and original-anchor provenance for a root comment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommentAnchor {
    pub file_path: String,
    pub start_line: Option<u32>,
    pub line: u32,
    pub commit_sha: String,
    pub original_start_line: Option<u32>,
    pub original_line: u32,
    pub original_commit_sha: String,
}

impl CommentAnchor {
    /// Construct and validate a new comment anchor at its initial commit.
    pub fn new(
        file_path: &str,
        start_line: Option<u32>,
        line: u32,
        commit_sha: &str,
    ) -> Result<Self> {
        let file_path = Self::validate_coordinates(file_path, start_line, line)?;
        let commit_sha = commit_sha.trim();
        if commit_sha.is_empty() {
            return Err(Error::Invalid(
                "comment anchor commit_sha must not be empty".to_owned(),
            ));
        }
        Ok(Self {
            file_path: file_path.to_owned(),
            start_line,
            line,
            commit_sha: commit_sha.to_owned(),
            original_start_line: start_line,
            original_line: line,
            original_commit_sha: commit_sha.to_owned(),
        })
    }

    // Public ------------------------------------------------------------------

    /// Validate in memory that `file_path` content exists and `1 <= start_line <= line <= total_lines`.
    pub fn validate_line(
        head_blob: Option<&str>,
        file_path: &str,
        start_line: Option<u32>,
        line: u32,
    ) -> Result<()> {
        let file_path = Self::validate_coordinates(file_path, start_line, line)?;
        let head_content = head_blob.ok_or_else(|| {
            Error::Invalid(format!(
                "file '{file_path}' does not exist at target commit"
            ))
        })?;
        let total_lines = u32::try_from(head_content.lines().count().max(1)).unwrap_or(u32::MAX);
        if line > total_lines {
            return Err(Error::Invalid(format!(
                "line {line} is out of range for '{file_path}' ({total_lines} lines)"
            )));
        }
        Ok(())
    }

    /// Recompute surviving and outdated line anchors across `old_blobs -> new_blobs` purely in memory.
    pub fn recompute_shifts(
        old_blobs: &BTreeMap<String, String>,
        new_blobs: &BTreeMap<String, String>,
        new_head_sha: &str,
        comments: &[Comment],
    ) -> Vec<CommentAnchorShift> {
        let mut shifts = Vec::new();
        for comment in comments {
            let Some(anchor) = &comment.anchor else {
                continue;
            };
            if comment.outdated {
                continue;
            }

            let Some(old_text) = old_blobs.get(&anchor.file_path) else {
                continue;
            };
            let Some(new_text) = new_blobs.get(&anchor.file_path) else {
                shifts.push(CommentAnchorShift {
                    comment_id: comment.id,
                    file_path: None,
                    start_line: anchor.start_line.and_then(|v| i32::try_from(v).ok()),
                    line: i32::try_from(anchor.line).ok(),
                    commit_sha: Some(anchor.commit_sha.clone()),
                    outdated: true,
                });
                continue;
            };

            if old_text == new_text {
                if anchor.commit_sha != new_head_sha {
                    shifts.push(CommentAnchorShift {
                        comment_id: comment.id,
                        file_path: None,
                        start_line: anchor.start_line.and_then(|v| i32::try_from(v).ok()),
                        line: i32::try_from(anchor.line).ok(),
                        commit_sha: Some(new_head_sha.to_owned()),
                        outdated: false,
                    });
                }
                continue;
            }

            let line_map = Self::compute_surviving_line_map(old_text, new_text);
            let mapped_line = line_map.get(&anchor.line).copied().flatten();
            let mapped_start = match anchor.start_line {
                Some(start) => line_map.get(&start).copied().flatten().map(Some),
                None => Some(None),
            };

            match (mapped_start, mapped_line) {
                (Some(new_start), Some(new_line)) => {
                    shifts.push(CommentAnchorShift {
                        comment_id: comment.id,
                        file_path: None,
                        start_line: new_start.and_then(|v| i32::try_from(v).ok()),
                        line: i32::try_from(new_line).ok(),
                        commit_sha: Some(new_head_sha.to_owned()),
                        outdated: false,
                    });
                }
                _ => {
                    shifts.push(CommentAnchorShift {
                        comment_id: comment.id,
                        file_path: None,
                        start_line: anchor.start_line.and_then(|v| i32::try_from(v).ok()),
                        line: i32::try_from(anchor.line).ok(),
                        commit_sha: Some(anchor.commit_sha.clone()),
                        outdated: true,
                    });
                }
            }
        }
        shifts
    }

    // Private -----------------------------------------------------------------

    fn validate_coordinates(file_path: &str, start_line: Option<u32>, line: u32) -> Result<&str> {
        let file_path = file_path.trim();
        if file_path.is_empty() {
            return Err(Error::Invalid(
                "comment anchor file_path must not be empty".to_owned(),
            ));
        }
        if line == 0 {
            return Err(Error::Invalid(
                "comment anchor line must be >= 1".to_owned(),
            ));
        }
        if let Some(start) = start_line
            && (start == 0 || start > line)
        {
            return Err(Error::Invalid(format!(
                "comment anchor start_line ({start}) must be between 1 and line ({line})"
            )));
        }
        Ok(file_path)
    }

    fn compute_surviving_line_map(old_text: &str, new_text: &str) -> BTreeMap<u32, Option<u32>> {
        let diff = TextDiff::from_lines(old_text, new_text);
        let mut map = BTreeMap::new();
        for change in diff.iter_all_changes() {
            match change.tag() {
                ChangeTag::Equal => {
                    if let (Some(old_idx), Some(new_idx)) = (change.old_index(), change.new_index())
                    {
                        let old_line = u32::try_from(old_idx + 1).unwrap_or(1);
                        let new_line = u32::try_from(new_idx + 1).unwrap_or(1);
                        map.insert(old_line, Some(new_line));
                    }
                }
                ChangeTag::Delete => {
                    if let Some(old_idx) = change.old_index() {
                        let old_line = u32::try_from(old_idx + 1).unwrap_or(1);
                        map.insert(old_line, None);
                    }
                }
                ChangeTag::Insert => {}
            }
        }
        map
    }
}

/// Application comment entity representing a general review comment, inline diff thread root, or reply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Comment {
    pub id: Uuid,
    pub number: u64,
    pub review_id: Uuid,
    pub session_id: Uuid,
    pub author: Identity,
    pub reply_to_number: Option<u64>,
    pub anchor: Option<CommentAnchor>,
    pub body: String,
    pub resolved: bool,
    pub outdated: bool,
    pub forge_id: Option<String>,
}

impl Comment {
    /// Convert a persisted database comment row into a domain `Comment`.
    pub fn from_db(
        model: database::Comment,
        author: Identity,
        reply_to_number: Option<u64>,
    ) -> Result<Self> {
        let number = u64::try_from(model.number)
            .map_err(|_| Error::Invalid("comment.number must be positive".to_owned()))?;
        let anchor = if let Some(file_path) = model.file_path {
            let line = model
                .line
                .ok_or_else(|| Error::Invalid("anchored comment is missing line".to_owned()))?;
            let commit_sha = model.commit_sha.ok_or_else(|| {
                Error::Invalid("anchored comment is missing commit_sha".to_owned())
            })?;
            let original_line = model.original_line.unwrap_or(line);
            let original_commit_sha = model
                .original_commit_sha
                .unwrap_or_else(|| commit_sha.clone());
            Some(CommentAnchor {
                file_path,
                start_line: model.start_line.and_then(|value| u32::try_from(value).ok()),
                line: u32::try_from(line)
                    .map_err(|_| Error::Invalid("comment.line must be positive".to_owned()))?,
                commit_sha,
                original_start_line: model
                    .original_start_line
                    .and_then(|value| u32::try_from(value).ok()),
                original_line: u32::try_from(original_line).map_err(|_| {
                    Error::Invalid("comment.original_line must be positive".to_owned())
                })?,
                original_commit_sha,
            })
        } else {
            None
        };

        Ok(Self {
            id: model.id,
            number,
            review_id: model.review_id,
            session_id: model.session_id,
            author,
            reply_to_number,
            anchor,
            body: model.body,
            resolved: model.resolved,
            outdated: model.outdated,
            forge_id: model.forge_id,
        })
    }

    // Public ------------------------------------------------------------------

    /// Format the Markdown body exported to a remote forge, appending an AI agent disclosure footer when authored by an `Agent`.
    pub fn format_export_body(&self) -> String {
        format_body_with_disclosure(&self.body, &self.author)
    }
}

impl From<CommentAnchor> for CommentAnchorDetail {
    fn from(anchor: CommentAnchor) -> Self {
        Self {
            file_path: anchor.file_path,
            start_line: anchor.start_line,
            line: anchor.line,
            commit_sha: anchor.commit_sha,
            original_start_line: anchor.original_start_line,
            original_line: anchor.original_line,
            original_commit_sha: anchor.original_commit_sha,
            ..Default::default()
        }
    }
}

impl From<Comment> for CommentDetail {
    fn from(comment: Comment) -> Self {
        Self {
            id: comment.id.to_string(),
            number: comment.number,
            review_id: comment.review_id.to_string(),
            session_id: comment.session_id.to_string(),
            author: Some(IdentitySummary::from(comment.author)).into(),
            reply_to_number: comment.reply_to_number,
            anchor: comment.anchor.map(CommentAnchorDetail::from).into(),
            body: comment.body,
            resolved: comment.resolved,
            outdated: comment.outdated,
            forge_id: comment.forge_id,
            ..Default::default()
        }
    }
}

/// Append the AI agent disclosure footer if the author is an `Agent` and the body does not already include it.
pub fn format_body_with_disclosure(body: &str, author: &Identity) -> String {
    let trimmed = body.trim();
    if let Some(footer) = author.agent_disclosure_footer() {
        if trimmed.contains(&footer) {
            return trimmed.to_owned();
        }
        if trimmed.is_empty() {
            return footer;
        }
        return format!("{trimmed}\n\n{footer}");
    }
    trimmed.to_owned()
}

/// Extract and strip an AI agent disclosure footer (`> Generated by AI agent \`<provider>/<model>\` via LGTM`) from a remote comment body.
pub fn parse_body_agent_disclosure(raw_body: &str) -> (String, Option<(String, String, String)>) {
    let trimmed = raw_body.trim();
    let prefix = "> Generated by AI agent `";
    let suffix = "` via LGTM";
    if let Some(idx) = trimmed.rfind(prefix) {
        let rest = &trimmed[idx + prefix.len()..];
        if let Some(descriptor) = rest.strip_suffix(suffix)
            && let Ok(parsed) = crate::utils::parse_agent_descriptor(descriptor)
        {
            let clean = trimmed[..idx].trim_end().to_owned();
            let body = if clean.is_empty() {
                trimmed.to_owned()
            } else {
                clean
            };
            return (body, Some(parsed));
        }
    }
    (trimmed.to_owned(), None)
}

// tests -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_append_agent_disclosure_footer_on_export() {
        let agent = Identity::new_agent(
            Uuid::new_v4(),
            "claude-sonnet-4-5",
            "anthropic",
            "claude-sonnet-4-5",
        )
        .unwrap();
        let exported = format_body_with_disclosure("Check error handling", &agent);
        assert_eq!(
            exported,
            "Check error handling\n\n> Generated by AI agent `anthropic/claude-sonnet-4-5` via LGTM"
        );
    }

    #[test]
    fn should_validate_line_and_recompute_shifts_in_memory() {
        let old_blob = "line 1\nline 2\nline 3\n";
        let new_blob = "header\nline 1\nmodified 2\nline 3\n";
        assert!(CommentAnchor::validate_line(Some(old_blob), "src/lib.rs", None, 2).is_ok());
        assert!(CommentAnchor::validate_line(Some(old_blob), "src/lib.rs", None, 99).is_err());

        let author = Identity::new_human(Uuid::new_v4(), "Ada", "ada@example.test", None).unwrap();
        let c_modified = Comment {
            id: Uuid::new_v4(),
            number: 1,
            review_id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            author: author.clone(),
            reply_to_number: None,
            anchor: Some(CommentAnchor::new("src/lib.rs", None, 2, "sha1").unwrap()),
            body: "Modified".to_owned(),
            resolved: false,
            outdated: false,
            forge_id: None,
        };
        let c_shifted = Comment {
            id: Uuid::new_v4(),
            number: 2,
            review_id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            author,
            reply_to_number: None,
            anchor: Some(CommentAnchor::new("src/lib.rs", None, 3, "sha1").unwrap()),
            body: "Shifted".to_owned(),
            resolved: false,
            outdated: false,
            forge_id: None,
        };

        let mut old_blobs = BTreeMap::new();
        old_blobs.insert("src/lib.rs".to_owned(), old_blob.to_owned());
        let mut new_blobs = BTreeMap::new();
        new_blobs.insert("src/lib.rs".to_owned(), new_blob.to_owned());

        let shifts = CommentAnchor::recompute_shifts(
            &old_blobs,
            &new_blobs,
            "sha2",
            &[c_modified, c_shifted],
        );
        assert_eq!(shifts.len(), 2);
        assert!(shifts[0].outdated);
        assert!(!shifts[1].outdated);
        assert_eq!(shifts[1].line, Some(4));
        assert_eq!(shifts[1].commit_sha.as_deref(), Some("sha2"));
    }
}
