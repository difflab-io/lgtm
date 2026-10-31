//! Domain model for inline diff comments, thread replies, and line anchors.

use uuid::Uuid;

use crate::{
    database,
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
        let file_path = file_path.trim();
        let commit_sha = commit_sha.trim();
        if file_path.is_empty() {
            return Err(Error::Invalid(
                "comment anchor file_path must not be empty".to_owned(),
            ));
        }
        if commit_sha.is_empty() {
            return Err(Error::Invalid(
                "comment anchor commit_sha must not be empty".to_owned(),
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
}
