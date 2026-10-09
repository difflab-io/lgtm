//! State owned by `ReviewView`. The composer now wraps a gpui-kit
//! `TextareaState` entity (via `review.rs`); only metadata and payload data
//! live here so this file stays free of UI library concerns.

use crate::proto::{AuthorKind, ChangedFile, Thread, User};

#[derive(Clone)]
pub struct ReviewConfig {
    pub backend: String,
    pub repo_path: String,
    pub base_ref: String,
}

impl ReviewConfig {
    pub fn from_env() -> Self {
        Self {
            backend: std::env::var("LGTM_ADDR")
                .map(|a| if a.starts_with("http") { a } else { format!("http://{a}") })
                .unwrap_or_else(|_| "http://127.0.0.1:50051".into()),
            repo_path: std::env::var("LGTM_REPO").unwrap_or_else(|_| {
                "/Users/siddharthkapoor/.herdr/worktrees/lgtm/poc-gpui-rpc-sqlite/.tmp/repos/ripgrep"
                    .into()
            }),
            base_ref: std::env::var("LGTM_BASE").unwrap_or_else(|_| "HEAD~5".into()),
        }
    }
}

/// What the composer is anchored to: a new thread on a diff line, or a reply
/// to an existing thread. Owned by `review::Composer` alongside the
/// `Entity<TextareaState>` that holds the editable text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComposerTarget {
    NewThread { line: i32 },
    Reply { thread_id: i64 },
}

/// One diff row plus its already-computed syntect spans. Caching avoids
/// re-running the highlighter per line on every unrelated re-render (e.g.
/// each composer keystroke used to re-highlight the whole file).
pub struct CachedDiffLine {
    pub kind: crate::diff::DiffLineKind,
    pub text: String,
    pub new_line: i32,
    /// `Some(spans)` when syntect produced highlighted runs for the code
    /// body; `None` for header / hunk / unknown-language lines, which the
    /// view renders as a single uncolored run.
    pub spans: Option<Vec<(u32, String)>>,
}

#[derive(Default)]
pub struct ReviewState {
    pub files: Vec<ChangedFile>,
    pub selected: Option<String>,
    pub diff_text: String,
    pub diff_lines: Vec<CachedDiffLine>,
    pub threads: Vec<Thread>,
    pub user: Option<User>,
    pub error: Option<String>,
    pub loading: bool,
}

impl ReviewState {
    pub fn selected_path(&self) -> Option<&str> {
        self.selected.as_deref()
    }

    pub fn threads_for_line(&self, line: i32) -> impl Iterator<Item = &Thread> {
        self.threads.iter().filter(move |t| t.line == line)
    }

    pub fn default_identity() -> (String, AuthorKind) {
        let name = std::env::var("LGTM_AUTHOR")
            .ok()
            .or_else(|| std::env::var("USER").ok())
            .unwrap_or_else(|| "you".into());
        let kind = match std::env::var("LGTM_AUTHOR_KIND").as_deref() {
            Ok("agent") => AuthorKind::Agent,
            _ => AuthorKind::User,
        };
        (name, kind)
    }
}
