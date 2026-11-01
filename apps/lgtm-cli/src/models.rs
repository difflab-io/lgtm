//! Shared CLI arguments, enum conversions, and JSON/terminal formatting helpers.

use clap::{Args, ValueEnum};
use serde_json::{Value, json};

use crate::backend::proto::lgtm::v1::{
    CommentAnchorDetail, CommentDetail, DiffHunkDetail, DiffLineKind, IdentitySummary,
    ReviewGroupBy as ProtoGroupBy, ReviewOutcome as ProtoOutcome, ReviewStatus, ReviewSummary,
    SessionState, SessionSummary,
};

#[derive(Args, Clone, Copy, Debug, Default)]
pub struct LocalArgs {
    /// Keep work within LGTM-managed local snapshot storage without mutating the checkout or remote forge.
    #[arg(long)]
    pub local: bool,
}

#[derive(Args, Clone, Debug, Default)]
pub struct ActorArgs {
    /// Attribute the operation to a named AI agent (`<provider>/<model>` or `<model>`).
    #[arg(long = "as", value_parser = parse_nonempty)]
    pub agent_name: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum ReviewOutcome {
    Comment,
    Reject,
    Approve,
    Request,
}

impl ReviewOutcome {
    // Public ------------------------------------------------------------------

    /// Return the lowercase wire label for this review outcome.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Comment => "comment",
            Self::Reject => "reject",
            Self::Approve => "approve",
            Self::Request => "request",
        }
    }
}

impl From<ReviewOutcome> for ProtoOutcome {
    fn from(outcome: ReviewOutcome) -> Self {
        match outcome {
            ReviewOutcome::Comment => Self::REVIEW_OUTCOME_COMMENT,
            ReviewOutcome::Reject => Self::REVIEW_OUTCOME_REJECT,
            ReviewOutcome::Approve => Self::REVIEW_OUTCOME_APPROVE,
            ReviewOutcome::Request => Self::REVIEW_OUTCOME_REQUEST,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum ReviewGroupBy {
    File,
    Session,
}

impl From<ReviewGroupBy> for ProtoGroupBy {
    fn from(group_by: ReviewGroupBy) -> Self {
        match group_by {
            ReviewGroupBy::File => Self::REVIEW_GROUP_BY_FILE,
            ReviewGroupBy::Session => Self::REVIEW_GROUP_BY_SESSION,
        }
    }
}

pub fn parse_nonempty(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        Err("Expected nonempty text".into())
    } else {
        Ok(value.into())
    }
}

pub fn parse_comment_id(value: &str) -> Result<u64, String> {
    let trimmed = value.trim().trim_start_matches(['C', 'c', '#']);
    trimmed
        .parse::<u64>()
        .ok()
        .filter(|&id| id > 0)
        .ok_or_else(|| format!("Invalid comment ID '{value}'; expected a positive number"))
}

pub fn short_sha(sha: &str) -> &str {
    let trimmed = sha.trim();
    if trimmed.len() > 7 {
        &trimmed[..7]
    } else {
        trimmed
    }
}

pub fn format_identity_display(identity: Option<&IdentitySummary>) -> String {
    let Some(id) = identity else {
        return "unknown".to_owned();
    };
    if id.kind.eq_ignore_ascii_case("agent") {
        format!("{} (agent)", id.name)
    } else {
        id.name.clone()
    }
}

pub fn format_identity_json(identity: Option<&IdentitySummary>) -> Value {
    let Some(id) = identity else {
        return Value::Null;
    };
    json!({
        "id": id.id,
        "name": id.name,
        "kind": id.kind,
        "email": id.email,
        "forge_login": id.forge_login,
        "provider": id.provider,
        "model": id.model,
    })
}

pub fn format_review_status(status: Option<ReviewStatus>) -> &'static str {
    match status {
        Some(ReviewStatus::REVIEW_STATUS_CLOSED) => "closed",
        Some(ReviewStatus::REVIEW_STATUS_MERGED) => "merged",
        _ => "open",
    }
}

pub fn format_session_state(state: Option<SessionState>) -> &'static str {
    match state {
        Some(SessionState::SESSION_STATE_PUBLISHED) => "published",
        _ => "draft",
    }
}

pub fn format_session_outcome(outcome: Option<ProtoOutcome>) -> Option<&'static str> {
    match outcome {
        Some(ProtoOutcome::REVIEW_OUTCOME_COMMENT) => Some("comment"),
        Some(ProtoOutcome::REVIEW_OUTCOME_APPROVE) => Some("approve"),
        Some(ProtoOutcome::REVIEW_OUTCOME_REQUEST) => Some("request"),
        Some(ProtoOutcome::REVIEW_OUTCOME_REJECT) => Some("reject"),
        _ => None,
    }
}

pub fn format_diff_line_kind(kind: Option<DiffLineKind>) -> (&'static str, char) {
    match kind {
        Some(DiffLineKind::DIFF_LINE_KIND_ADDED) => ("added", '+'),
        Some(DiffLineKind::DIFF_LINE_KIND_REMOVED) => ("removed", '-'),
        _ => ("context", ' '),
    }
}

pub fn format_review_json(review: &ReviewSummary) -> Value {
    json!({
        "id": review.id,
        "number": review.number,
        "name": review.name,
        "local": review.local,
        "git_dir": review.git_dir,
        "branch": review.branch,
        "repository_url": review.repository_url,
        "pr_number": review.pr_number,
        "forge_id": review.forge_id,
        "status": format_review_status(review.status.as_known()),
        "base_sha": review.base_sha,
        "head_sha": review.head_sha,
        "reviewed_files": review.reviewed_files,
    })
}

pub fn format_session_json(session: &SessionSummary) -> Value {
    json!({
        "id": session.id,
        "review_id": session.review_id,
        "number": session.number,
        "author": format_identity_json(session.author.as_option()),
        "state": format_session_state(session.state.as_known()),
        "outcome": format_session_outcome(session.outcome.as_known()),
        "body": session.body,
        "forge_id": session.forge_id,
        "published_at": session.published_at,
    })
}

pub fn format_anchor_json(anchor: &CommentAnchorDetail) -> Value {
    json!({
        "file_path": anchor.file_path,
        "start_line": anchor.start_line,
        "line": anchor.line,
        "commit_sha": anchor.commit_sha,
        "original_start_line": anchor.original_start_line,
        "original_line": anchor.original_line,
        "original_commit_sha": anchor.original_commit_sha,
    })
}

pub fn format_comment_json(comment: &CommentDetail) -> Value {
    json!({
        "id": comment.id,
        "number": comment.number,
        "review_id": comment.review_id,
        "session_id": comment.session_id,
        "author": format_identity_json(comment.author.as_option()),
        "reply_to_number": comment.reply_to_number,
        "anchor": comment.anchor.as_option().map(format_anchor_json),
        "body": comment.body,
        "resolved": comment.resolved,
        "outdated": comment.outdated,
        "forge_id": comment.forge_id,
    })
}

pub fn format_diff_hunk_json(hunk: &DiffHunkDetail) -> Value {
    let lines: Vec<Value> = hunk
        .lines
        .iter()
        .map(|line| {
            let (kind, _) = format_diff_line_kind(line.kind.as_known());
            json!({
                "kind": kind,
                "line_number": line.line_number,
                "content": line.content,
            })
        })
        .collect();
    json!({
        "file_path": hunk.file_path,
        "commit_sha": hunk.commit_sha,
        "start_line": hunk.start_line,
        "lines": lines,
    })
}
