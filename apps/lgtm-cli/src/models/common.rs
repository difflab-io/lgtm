//! Shared CLI argument structs, parsers, and view formatting primitives.

use clap::{Args, ValueEnum};
use lgtm_forge::proto::lgtm::v1::{
    CommentAnchorDetail, DiffLineKind, IdentitySummary, ReviewGroupBy as ProtoGroupBy,
    ReviewOutcome as ProtoOutcome, ReviewStatus, SessionState,
};
use serde_json::{Value, json};

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

/// Human or AI agent identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub email: Option<String>,
    pub forge_login: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
}

impl Identity {
    // Public ------------------------------------------------------------------

    /// Render the display label for terminal output (`<name>` for humans, `<name> (agent)` for agents).
    pub fn display_label(&self) -> String {
        if self.kind.eq_ignore_ascii_case("agent") {
            format!("{} (agent)", self.name)
        } else {
            self.name.clone()
        }
    }

    /// Serialize this identity to its JSON contract.
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "kind": self.kind,
            "email": self.email,
            "forge_login": self.forge_login,
            "provider": self.provider,
            "model": self.model,
        })
    }
}

impl From<&IdentitySummary> for Identity {
    fn from(id: &IdentitySummary) -> Self {
        Self {
            id: id.id.clone(),
            name: id.name.clone(),
            kind: id.kind.clone(),
            email: id.email.clone(),
            forge_login: id.forge_login.clone(),
            provider: id.provider.clone(),
            model: id.model.clone(),
        }
    }
}

/// File and line diff anchor for a comment.
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
    // Public ------------------------------------------------------------------

    /// Format the human-readable line or line-range label (`line N` or `lines M-N`).
    pub fn line_range_label(&self) -> String {
        match self.start_line {
            Some(start) if start < self.line => format!("lines {start}-{}", self.line),
            _ => format!("line {}", self.line),
        }
    }

    /// Format the compact `<file>:<line>` or `<file>:<start>-<line>` label.
    pub fn file_line_label(&self) -> String {
        let line_part = match self.start_line {
            Some(start) if start < self.line => format!("{start}-{}", self.line),
            _ => self.line.to_string(),
        };
        format!("{}:{line_part}", self.file_path)
    }

    /// Serialize this anchor to its JSON contract.
    pub fn to_json(&self) -> Value {
        json!({
            "file_path": self.file_path,
            "start_line": self.start_line,
            "line": self.line,
            "commit_sha": self.commit_sha,
            "original_start_line": self.original_start_line,
            "original_line": self.original_line,
            "original_commit_sha": self.original_commit_sha,
        })
    }
}

impl From<&CommentAnchorDetail> for CommentAnchor {
    fn from(anchor: &CommentAnchorDetail) -> Self {
        Self {
            file_path: anchor.file_path.clone(),
            start_line: anchor.start_line,
            line: anchor.line,
            commit_sha: anchor.commit_sha.clone(),
            original_start_line: anchor.original_start_line,
            original_line: anchor.original_line,
            original_commit_sha: anchor.original_commit_sha.clone(),
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

pub fn format_section_banner(title: &str) -> String {
    format_banner(&format!("══ {title} "), '═')
}

pub fn format_sub_banner(title: &str) -> String {
    format_banner(&format!("── {title} "), '─')
}

fn format_banner(prefix: &str, glyph: char) -> String {
    let pad = 72usize.saturating_sub(prefix.chars().count());
    format!("{prefix}{}", glyph.to_string().repeat(pad.max(3)))
}
