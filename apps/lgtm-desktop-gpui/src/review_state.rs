//! State owned by `ReviewView`. Pulled into its own module so the GPUI
//! rendering code in `review.rs` stays about layout, not transport.

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
                .map(|a| {
                    if a.starts_with("http") {
                        a
                    } else {
                        format!("http://{a}")
                    }
                })
                .unwrap_or_else(|_| "http://127.0.0.1:50051".into()),
            repo_path: std::env::var("LGTM_REPO").unwrap_or_else(|_| {
                "/Users/siddharthkapoor/.herdr/worktrees/lgtm/poc-gpui-rpc-sqlite/.tmp/repos/ripgrep"
                    .into()
            }),
            base_ref: std::env::var("LGTM_BASE").unwrap_or_else(|_| "HEAD~5".into()),
        }
    }
}

#[derive(Default)]
pub struct ReviewState {
    pub files: Vec<ChangedFile>,
    pub selected: Option<String>,
    pub diff_text: String,
    pub threads: Vec<Thread>,
    pub user: Option<User>,
    pub error: Option<String>,
    pub loading: bool,
    // Composer: when `Some`, show a draft editor for a new thread at this line.
    pub composing_line: Option<i32>,
    pub composer_body: String,
    // Separate reply composers keyed by thread id.
    pub replying_to: Option<i64>,
    pub reply_body: String,
}

impl ReviewState {
    pub fn selected_path(&self) -> Option<&str> {
        self.selected.as_deref()
    }

    pub fn threads_for_line(&self, line: i32) -> impl Iterator<Item = &Thread> {
        self.threads.iter().filter(move |t| t.line == line)
    }

    /// Default identity shown in the status line and used when nothing is
    /// configured. Users override by setting `LGTM_AUTHOR` / `LGTM_AUTHOR_KIND`.
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
