use super::{Comment, Context, Session, Thread};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Conversation {
    pub context: Context,
    pub sessions: Vec<Session>,
    pub threads: Vec<Thread>,
    pub comments: Vec<Comment>,
}

impl Conversation {
    // Public ------------------------------------------------------------------

    // Public: Export ----------------------------------------------------------

    /// Format selected discussion as feedback a person or agent can act on.
    /// Original locations remain useful when a current placement cannot be confirmed.
    pub fn export_feedback(&self, session: &crate::models::Session) -> String {
        let conversation = self;
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
        let observed_overall = conversation.comments.iter().find(|comment| {
            comment.forge_object_kind.as_deref() == Some("review")
                && comment.forge_review_id == session.forge_review_id
                && session.forge_review_id.is_some()
        });
        let overall = if let Some(comment) = observed_overall {
            Some(without_receipt(comment, &conversation.sessions))
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
                !conversation.comments.iter().any(|other| {
                    other.thread_id == comment.thread_id && other.position < comment.position
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
                .filter(|reply| reply.thread_id == thread.id && reply.position > comment.position)
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
}
