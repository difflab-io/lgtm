//! `ReviewView` — top-level entity that composes the three-pane layout.
//!
//! It is deliberately thin:
//! - the `FileTree` entity owns the left pane (file list),
//! - the `DiffView` entity owns the middle pane (diff + inline threads + composer),
//! - the threads sidebar + identity badge are rendered inline here (they only
//!   update when `DiffView` notifies — not per keystroke).

use std::sync::Arc;

use anyhow::Result;
use gpui_kit::*;

use crate::client::LgtmClient;
use crate::diff_view::DiffView;
use crate::file_tree::FileTree;
use crate::proto::{AuthorKind, ChangedFile, User};
use crate::review_state::{ReviewConfig, ReviewState};

const FG: u32 = 0xd7dbe0;
const MUTED: u32 = 0x8b94a3;
const BORDER: u32 = 0x1e2228;
const BG: u32 = 0x0b0d10;
const ACCENT: u32 = 0x4c9aff;
const CARD_BG: u32 = 0x121520;
const AGENT: u32 = 0xf59f00;

pub struct ReviewView {
    config: ReviewConfig,
    client: Arc<LgtmClient>,
    state: ReviewState,
    diff_view: Entity<DiffView>,
    file_tree: Entity<FileTree>,
    _diff_subscription: Subscription,
}

impl ReviewView {
    pub fn new(config: ReviewConfig, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let client = Arc::new(LgtmClient::new(config.backend.clone()));
        let diff_view = cx.new(|_| DiffView::new(config.clone(), client.clone()));
        let file_tree = cx.new(|_| FileTree::new(diff_view.clone()));

        // ReviewView only re-renders when DiffView notifies (which happens on
        // real state changes: file selection, thread create/resolve/reply,
        // identity resolved — *not* per Textarea keystroke).
        let sub = cx.observe(&diff_view, |_, _, cx| cx.notify());

        let mut this = Self {
            config: config.clone(),
            client: client.clone(),
            state: ReviewState::default(),
            diff_view,
            file_tree,
            _diff_subscription: sub,
        };
        this.spawn_initial_load(cx);
        this
    }

    fn spawn_initial_load(&mut self, cx: &mut Context<Self>) {
        self.state.loading = true;
        let client = self.client.clone();
        let repo = self.config.repo_path.clone();
        let base = self.config.base_ref.clone();
        let (name, kind) = ReviewState::default_identity();

        cx.spawn(async move |this, cx| {
            let result: Result<(Vec<ChangedFile>, User)> = cx
                .background_spawn(async move {
                    let user = client.upsert_user(&name, kind)?;
                    let files = client.get_changed_files(&repo, &base)?;
                    Ok((files, user))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.state.loading = false;
                match result {
                    Ok((files, user)) => {
                        this.state.user = Some(user.clone());
                        this.diff_view.update(cx, |d, cx| d.set_user(user, cx));
                        let files_clone = files.clone();
                        this.file_tree.update(cx, |t, cx| {
                            t.set_files(files_clone, cx);
                            t.select_initial(cx);
                        });
                        this.state.files = files;
                    }
                    Err(e) => this.state.error = Some(format!("{e:#}")),
                }
                cx.notify();
            });
        })
        .detach();
    }
}

impl Render for ReviewView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if std::env::var("LGTM_TRACE_RENDER").is_ok() {
            use std::sync::atomic::{AtomicU64, Ordering};
            static N: AtomicU64 = AtomicU64::new(0);
            let n = N.fetch_add(1, Ordering::Relaxed) + 1;
            eprintln!("[lgtm-gpui] ReviewView render #{n}");
        }
        div()
            .flex()
            .flex_col()
            .w_full()
            .h_full()
            .bg(rgb(BG))
            .text_color(rgb(FG))
            .text_size(px(13.0))
            .child(div().h(px(28.)).flex_shrink_0())
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_grow(1.0)
                    .min_h(px(0.))
                    .child(self.file_tree.clone())
                    .child(self.diff_view.clone())
                    .child(self.render_threads_panel(cx)),
            )
    }
}

impl ReviewView {
    fn render_threads_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let snapshot = self.diff_view.read(cx).snapshot();
        let threads = snapshot.threads;

        let mut cards = Vec::<AnyElement>::with_capacity(threads.len());
        for t in &threads {
            cards.push(render_thread_card_readonly(t).into_any_element());
        }
        let count = threads.len();

        let identity = self
            .state
            .user
            .as_ref()
            .map(|u| {
                let kind = match u.kind {
                    AuthorKind::Agent => "agent",
                    _ => "user",
                };
                SharedString::from(format!("as {} {}", kind, u.name))
            })
            .unwrap_or_else(|| "identifying…".into());

        let empty_hint = if cards.is_empty() {
            Some(
                div()
                    .px_3()
                    .py_2()
                    .text_color(rgb(MUTED))
                    .child("No threads for this file"),
            )
        } else {
            None
        };

        div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .w(px(360.))
            .h_full()
            .border_l_1()
            .border_color(rgb(BORDER))
            .child(section_header(SharedString::from(format!("THREADS · {count}"))))
            .child(
                div()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .text_size(px(11.))
                    .text_color(rgb(MUTED))
                    .child(identity),
            )
            .child(
                div()
                    .id("threads-scroller")
                    .flex_grow(1.0)
                    .flex()
                    .flex_col()
                    .overflow_y_scroll()
                    .p_2()
                    .children(empty_hint)
                    .children(cards),
            )
    }
}

fn section_header(text: impl Into<SharedString>) -> impl IntoElement {
    div()
        .px_3()
        .py_2()
        .border_b_1()
        .border_color(rgb(BORDER))
        .text_color(rgb(MUTED))
        .text_size(px(11.))
        .font_weight(FontWeight::SEMIBOLD)
        .child(text.into())
}

fn render_thread_card_readonly(thread: &crate::proto::Thread) -> impl IntoElement {
    let state = if thread.resolved { "resolved" } else { "open" };
    let state_color = if thread.resolved { MUTED } else { ACCENT };
    let header_text = SharedString::from(format!(
        "Thread #{} · {}{}  · {}",
        thread.id,
        thread.file_path,
        if thread.line > 0 { format!(":{}", thread.line) } else { "".into() },
        state,
    ));

    let comments: Vec<AnyElement> = thread
        .comments
        .iter()
        .map(|c| render_comment(c).into_any_element())
        .collect();

    div()
        .flex()
        .flex_col()
        .gap_1()
        .p_2()
        .mb_1()
        .bg(rgb(CARD_BG))
        .border_1()
        .border_color(rgb(BORDER))
        .rounded(px(6.))
        .child(
            div()
                .text_size(px(11.))
                .text_color(rgb(state_color))
                .child(header_text),
        )
        .child(div().flex().flex_col().gap_1().children(comments))
}

fn render_comment(c: &crate::proto::Comment) -> impl IntoElement {
    let kind_label = match c.author.kind {
        AuthorKind::Agent => "agent",
        _ => "user",
    };
    let kind_color = match c.author.kind {
        AuthorKind::Agent => AGENT,
        _ => ACCENT,
    };
    let header = SharedString::from(format!("{} · {}", c.author.name, kind_label));
    let body = SharedString::from(c.body.clone());

    div()
        .flex()
        .flex_col()
        .gap_1()
        .p_2()
        .rounded(px(4.))
        .bg(rgb(0x0e1119))
        .child(
            div()
                .text_size(px(10.))
                .text_color(rgb(kind_color))
                .child(header),
        )
        .child(div().text_color(rgb(FG)).child(body))
}
