//! Shared formatting helpers and forge import/export contracts.
use crate::models::{Anchor, ForgeObservation};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub fn within(path: &str, root: &str) -> bool {
    Path::new(path).starts_with(Path::new(root))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Forge account details supplied before matching a local actor.
pub struct RemoteAuthor {
    pub name: String,
    pub email: String,
    pub account_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Fetched forge comment awaiting reconciliation with a local comment.
pub struct RemoteComment {
    /// Private pending-review comment visible only to the posting account.
    pub pending: bool,
    pub id: String,
    pub kind: String,
    pub review_id: Option<String>,
    pub body: String,
    pub updated_at: String,
    pub author: RemoteAuthor,
    pub reply_to: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Fetched thread and comments awaiting reconciliation.
pub struct RemoteThread {
    pub id: String,
    pub anchor: Anchor,
    /// Provider current location, separate from the immutable original anchor.
    #[serde(default)]
    pub observation: Option<ForgeObservation>,
    pub resolved: bool,
    /// A provider-confirmed missing source file; unknown placement alone is not deletion.
    #[serde(default)]
    pub orphan_reason: Option<crate::models::OrphanReason>,
    pub comments: Vec<RemoteComment>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Complete or partial fetch result used to refresh a forge context.
pub struct ForgeSnapshot {
    pub complete: bool,
    #[serde(default)]
    pub state: Option<crate::database::types::ForgeState>,
    pub head: String,
    pub base: Option<String>,
    pub threads: Vec<RemoteThread>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Provider confirmation identifying a successfully posted comment.
pub struct PostReceipt {
    pub comment_id: String,
    pub kind: String,
    pub thread_id: Option<String>,
    pub review_id: Option<String>,
    pub posting_account_id: String,
}

/// A UTF-8 text file at the selected content revision.
#[derive(Clone, Debug)]
pub struct WorkspaceFile {
    pub path: String,
    /// Original repository path for renamed remote files; empty for local snapshots.
    pub previous_path: String,
    pub content: String,
    pub content_hash: String,
    pub status: String,
}

/// Picker entry containing immutable reviewed revision IDs.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PullRequest {
    pub number: i64,
    pub state: crate::database::types::ForgeState,
    pub title: String,
    pub head: String,
    pub base: String,
    pub repository_url: String,
    /// Provider account that opened this PR; absent in legacy import contracts.
    #[serde(default)]
    pub author_account_id: Option<String>,
}

/// Hash a domain response without relying on timestamps to distinguish concurrent edits.
pub fn object_hash<T: Serialize>(value: &T) -> crate::error::Result<String> {
    use sha2::{Digest, Sha256};
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(value)?)))
}

/// Opaque to consumers. Saved source identity is separate from this review-data token.
pub fn object_version<T: Serialize>(
    kind: crate::models::ObjectKind,
    version: i64,
    id: uuid::Uuid,
    value: &T,
) -> crate::error::Result<String> {
    Ok(format!(
        "{}:{version}:{id}:{}",
        kind.as_str(),
        object_hash(value)?
    ))
}

/// Format selected discussion as feedback a person or agent can act on.
/// Original locations remain useful when a current placement cannot be confirmed.
pub fn export_feedback(
    conversation: &crate::models::Conversation,
    session: &crate::models::Session,
) -> String {
    use crate::models::{Anchor, OrphanReason};
    fn code(value: &str) -> String {
        let value = value.replace('\r', "\\r").replace('\n', "\\n");
        let run = value.split(|c| c != '`').map(str::len).max().unwrap_or(0) + 1;
        let ticks = "`".repeat(run);
        let padding = if value.starts_with('`') || value.ends_with('`') {
            " "
        } else {
            ""
        };
        format!("{ticks}{padding}{value}{padding}{ticks}")
    }
    fn location(anchor: &Anchor) -> String {
        match anchor {
            Anchor::General => "Review-level feedback".into(),
            Anchor::File { path } => code(path),
            Anchor::Line {
                path,
                start,
                end,
                side,
                ..
            } => {
                let old = if side == "old" { "~" } else { "" };
                code(&if start == end {
                    format!("{path}:{old}{start}")
                } else {
                    format!("{path}:{old}{start}-{old}{end}")
                })
            }
        }
    }
    fn body(out: &mut String, text: &str, prefix: &str) {
        for line in text.split('\n') {
            out.push_str(prefix);
            out.push_str(line);
            out.push('\n');
        }
    }
    fn text(value: &str) -> String {
        let mut out = String::new();
        for c in value.chars() {
            if "\\`*_[]<>".contains(c) {
                out.push('\\');
            }
            out.push(c);
        }
        out
    }
    fn without_receipt(
        comment: &crate::models::Comment,
        sessions: &[crate::models::Session],
    ) -> String {
        let mut value = comment.body.clone();
        if let Some(origin) = comment.origin_session_id {
            let marker = format!("\n\n<!-- lgtm-comment:{origin}:{} -->", comment.id);
            if let Some(body) = value.strip_suffix(&marker) {
                value = body.to_owned();
                if comment.author.model().is_some() {
                    let footer = format!("\n\n> {}", comment.author.label());
                    if let Some(body) = value.strip_suffix(&footer) {
                        value = body.to_owned();
                    }
                }
            }
        }
        if comment.forge_object_kind.as_deref() == Some("review")
            && let Some(review) = sessions.iter().find(|review| {
                review.forge_review_id == comment.forge_review_id
                    && review.forge_review_id.is_some()
            })
        {
            let marker = format!("\n\n<!-- lgtm-review:{} -->", review.id);
            return value.strip_suffix(&marker).unwrap_or(&value).to_owned();
        }
        value
    }
    fn visible(comment: &crate::models::Comment) -> bool {
        !comment.deleted && comment.remote_presence != "missing"
    }
    let observed_overall = conversation.comments.iter().find(|comment| {
        comment.forge_object_kind.as_deref() == Some("review")
            && comment.forge_review_id == session.forge_review_id
            && session.forge_review_id.is_some()
    });
    let overall = if let Some(comment) = observed_overall {
        visible(comment).then(|| without_receipt(comment, &conversation.sessions))
    } else {
        session
            .completion()
            .and_then(|completion| completion.body.clone())
    }
    .filter(|body| !body.trim().is_empty());
    let overall = overall.as_deref();
    let mut roots: Vec<_> = conversation
        .comments
        .iter()
        .filter(|comment| {
            visible(comment)
                && !conversation.comments.iter().any(|other| {
                    other.thread_id == comment.thread_id
                        && visible(other)
                        && other.position < comment.position
                })
        })
        .collect();
    roots.retain(|comment| {
        !(overall.is_some()
            && comment.forge_object_kind.as_deref() == Some("review")
            && comment.forge_review_id == session.forge_review_id
            && session.forge_review_id.is_some())
    });
    roots.sort_by_key(|comment| {
        let anchor = conversation
            .threads
            .iter()
            .find(|thread| thread.id == comment.thread_id)
            .map(|thread| &thread.anchor);
        let (path, line) = match anchor {
            Some(Anchor::File { path }) => (path.as_str(), 0),
            Some(Anchor::Line { path, start, .. }) => (path.as_str(), *start),
            _ => ("", 0),
        };
        (
            path.to_owned(),
            line,
            comment.created_at.clone(),
            comment.id,
        )
    });
    let old_side = roots.iter().any(|comment| {
        conversation.threads.iter().any(|thread| {
            thread.id == comment.thread_id
                && matches!(&thread.anchor,Anchor::Line{side,..} if side=="old")
        })
    });
    let mut out = String::from("# Review feedback\n\n");
    if !roots.is_empty() {
        out.push_str(
            "Please address the following comments.\nLocations refer to the reviewed source; line numbers can differ in the current file.\n",
        );
        if old_side {
            out.push_str("Lines prefixed with ~ refer to the old (left) side of the diff.\n");
        }
        out.push('\n');
    }
    for (index, comment) in roots.iter().enumerate() {
        let Some(thread) = conversation
            .threads
            .iter()
            .find(|thread| thread.id == comment.thread_id)
        else {
            continue;
        };
        let current = thread
            .forge_observation
            .as_ref()
            .filter(|observed| {
                !observed.outdated
                    && Some(&observed.base) == session.commits.base.as_ref()
                    && Some(&observed.head) == session.commits.head.as_ref()
            })
            .and_then(|observed| observed.current.as_ref());
        let mut displayed = current.cloned().unwrap_or_else(|| thread.anchor.clone());
        let original = location(&thread.anchor);
        let moved = thread
            .current_path
            .as_ref()
            .is_some_and(|current| match &thread.anchor {
                Anchor::File { path } | Anchor::Line { path, .. } => {
                    current != thread.source_path.as_ref().unwrap_or(path)
                }
                Anchor::General => false,
            });
        let provider_outdated = thread
            .forge_observation
            .as_ref()
            .is_some_and(|observed| observed.outdated);
        let hint = match thread.orphan_reason {
            Some(OrphanReason::FileDeleted) => {
                Some(format!("File deleted. Original location: {original}."))
            }
            Some(OrphanReason::WorkingSourceChanged) => Some(format!(
                "Unstaged source changed. Original location: {original}."
            )),
            None if provider_outdated => Some(format!(
                "Outdated on the current PR. Original location: {original}."
            )),
            None if moved && current.is_none() => {
                displayed = Anchor::File {
                    path: thread.current_path.clone().unwrap_or_default(),
                };
                Some(format!(
                    "Moved from {original}. The current line placement is not verified."
                ))
            }
            None if thread.evidence.is_none()
                && current.is_none()
                && matches!(thread.anchor, Anchor::Line { .. }) =>
            {
                Some("Original location is not verified.".into())
            }
            _ => None,
        };
        let label = if session.comparison_kind == crate::models::ComparisonKind::Notes {
            "NOTE"
        } else {
            "COMMENT"
        };
        let resolved = if thread.state == "resolved" {
            " (resolved)"
        } else {
            ""
        };
        let value = without_receipt(comment, &conversation.sessions);
        let mut lines = value.split('\n');
        out.push_str(&format!(
            "{}. **[{label}]** {}{resolved} - {}\n",
            index + 1,
            location(&displayed),
            format_args!(
                "**{}:** {}",
                text(&comment.author.name),
                lines.next().unwrap_or_default()
            )
        ));
        let indent = " ".repeat((index + 1).to_string().len() + 2);
        for line in lines {
            out.push_str(&indent);
            out.push_str(line);
            out.push('\n');
        }
        if let Some(hint) = hint {
            out.push('\n');
            body(&mut out, &hint, &indent);
            if let Some(excerpt) = thread
                .evidence
                .as_ref()
                .map(|evidence| evidence.selected_text.as_str())
                .or(thread.selected_text.as_deref())
                .filter(|excerpt| !excerpt.is_empty())
            {
                out.push('\n');
                for line in excerpt.lines().take(12) {
                    out.push_str(&indent);
                    out.push_str("> ");
                    out.push_str(line);
                    out.push('\n');
                }
            }
        }
        let mut replies: Vec<_> = conversation
            .comments
            .iter()
            .filter(|reply| {
                reply.thread_id == thread.id && reply.position > comment.position && visible(reply)
            })
            .collect();
        replies.sort_by_key(|reply| (reply.position, reply.id));
        for reply in replies {
            out.push('\n');
            let value = without_receipt(reply, &conversation.sessions);
            let mut lines = value.split('\n');
            out.push_str(&format!(
                "{indent}**Reply — {}:** {}\n",
                text(&reply.author.name),
                lines.next().unwrap_or_default()
            ));
            for line in lines {
                out.push_str(&indent);
                out.push_str(line);
                out.push('\n');
            }
        }
        out.push('\n');
    }
    if let Some(overall) = overall {
        out.push_str("## Overall feedback\n\n");
        out.push_str(overall);
        out.push('\n');
    } else if roots.is_empty() {
        out.push_str("No feedback yet.\n");
    }
    out
}
