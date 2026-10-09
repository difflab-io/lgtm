//! Three-pane review view: file list (left), diff + inline threads (middle),
//! threads sidebar (right). Mirrors the SPA layout and class semantics.
//!
//! Interaction model:
//! - Clicking a changed file loads its diff + threads from the server.
//! - Clicking "Resolve" on a thread calls the server (no text input needed).
//! - New-thread / reply creation is intentionally delegated to the `lgtm`
//!   CLI in this PoC; a text-input widget in GPUI would require either the
//!   full `editor` crate or a hand-rolled input, both significantly bigger
//!   than this experiment allows. The "+" buttons copy a prefilled CLI
//!   command to the clipboard so the agent / human can paste it elsewhere.
//!
//! All background work runs via `cx.background_spawn`, which gives us a
//! thread pool outside the event loop. Results are applied back onto the
//! view via `this.update(...)` to trigger a re-render.

use std::sync::Arc;

use anyhow::Result;
use gpui::{
    App, AppContext, ClipboardItem, Context, FontWeight, InteractiveElement, IntoElement,
    ParentElement, Render, SharedString, StatefulInteractiveElement, Styled, Window, div,
    prelude::FluentBuilder, px, rgb,
};

use crate::client::LgtmClient;
use crate::diff::{DiffLine, DiffLineKind, parse_diff};
use crate::highlight::{Highlighter, lang_for_path};
use crate::proto::{AuthorKind, ChangedFile, Thread, User};
use crate::review_state::{ReviewConfig, ReviewState};

// ---- Colour tokens, matching the SPA palette. ----
const BG: u32 = 0x0b0d10;
const FG: u32 = 0xd7dbe0;
const MUTED: u32 = 0x8b94a3;
const BORDER: u32 = 0x1e2228;
const ACCENT: u32 = 0x4c9aff;
const ADD_BG: u32 = 0x183321;
const ADD_FG: u32 = 0xa7e7b4;
const REM_BG: u32 = 0x33181a;
const REM_FG: u32 = 0xf0a9a9;
const HUNK_BG: u32 = 0x15223a;
const AGENT: u32 = 0xf59f00;

pub struct ReviewView {
    config: ReviewConfig,
    client: Arc<LgtmClient>,
    state: ReviewState,
}

impl ReviewView {
    pub fn new(config: ReviewConfig, cx: &mut Context<Self>) -> Self {
        let client = Arc::new(LgtmClient::new(config.backend.clone()));
        let mut this = Self { config: config.clone(), client: client.clone(), state: ReviewState::default() };
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
                        this.state.files = files;
                        this.state.user = Some(user);
                        if let Some(first) = this.state.files.first().cloned() {
                            this.select_file(first.path, cx);
                        }
                    }
                    Err(e) => this.state.error = Some(format!("{e:#}")),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn select_file(&mut self, path: String, cx: &mut Context<Self>) {
        self.state.selected = Some(path.clone());
        self.state.diff_text.clear();
        self.state.threads.clear();
        self.state.loading = true;
        cx.notify();

        let client = self.client.clone();
        let repo = self.config.repo_path.clone();
        let base = self.config.base_ref.clone();
        let path_for_fetch = path.clone();
        cx.spawn(async move |this, cx| {
            let result: Result<(String, Vec<Thread>)> = cx
                .background_spawn(async move {
                    let diff = client.get_file_diff(&repo, &base, &path_for_fetch)?;
                    let threads = client.list_threads(&repo, &base, Some(&path_for_fetch))?;
                    Ok((diff, threads))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.state.loading = false;
                match result {
                    Ok((diff, threads)) => {
                        this.state.diff_text = diff;
                        this.state.threads = threads;
                    }
                    Err(e) => this.state.error = Some(format!("{e:#}")),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn resolve_thread(&mut self, thread_id: i64, cx: &mut Context<Self>) {
        let client = self.client.clone();
        cx.spawn(async move |this, cx| {
            let result: Result<Thread> = cx
                .background_spawn(async move { client.resolve_thread(thread_id) })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(updated) => {
                        for t in this.state.threads.iter_mut() {
                            if t.id == updated.id {
                                *t = updated.clone();
                            }
                        }
                    }
                    Err(e) => this.state.error = Some(format!("{e:#}")),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Copy a ready-to-run `lgtm comment add` command to the clipboard so
    /// the reviewer can post from a terminal. See the module docstring for
    /// why GPUI stops short of a built-in text input.
    fn copy_new_thread_hint(&self, line: i32, cx: &mut App) {
        let Some(path) = self.state.selected_path() else { return };
        let cmd = format!(
            "lgtm comment add {} --path {} --line {} --base {} --body '…'",
            shell_escape(&self.config.repo_path),
            shell_escape(path),
            line,
            shell_escape(&self.config.base_ref),
        );
        cx.write_to_clipboard(ClipboardItem::new_string(cmd));
    }

    fn copy_reply_hint(&self, thread_id: i64, cx: &mut App) {
        let cmd = format!("lgtm comment reply --thread {thread_id} --body '…'");
        cx.write_to_clipboard(ClipboardItem::new_string(cmd));
    }

    fn file_name(&self) -> SharedString {
        self.state
            .selected_path()
            .map(|p| SharedString::from(p.to_string()))
            .unwrap_or_else(|| "Select a file".into())
    }
}

impl Render for ReviewView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .w_full()
            .h_full()
            .bg(rgb(BG))
            .text_color(rgb(FG))
            .text_size(px(13.0))
            .child(self.render_sidebar(cx))
            .child(self.render_diff_and_threads(cx))
            .child(self.render_threads_panel(cx))
    }
}

impl ReviewView {
    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut items = Vec::with_capacity(self.state.files.len());
        for (i, file) in self.state.files.iter().enumerate() {
            items.push(self.render_file_item(i, file, cx));
        }
        div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .w(px(280.))
            .h_full()
            .border_r_1()
            .border_color(rgb(BORDER))
            .child(section_header("CHANGED FILES"))
            .child(div().flex().flex_col().py_1().children(items))
    }

    fn render_file_item(
        &self,
        idx: usize,
        file: &ChangedFile,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let is_active = self.state.selected.as_deref() == Some(&file.path);
        let path = SharedString::from(file.path.clone());
        let status_label = SharedString::from(file.status.clone());
        let status_fg = status_color(&file.status);
        let path_for_click = file.path.clone();

        div()
            .id(("file", idx))
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .px_3()
            .py_1()
            .cursor_pointer()
            .when(is_active, |d| d.bg(rgb(0x172238)))
            .hover(|d| d.bg(rgb(0x11161e)))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.select_file(path_for_click.clone(), cx);
            }))
            .child(
                div()
                    .w(px(72.))
                    .text_size(px(10.))
                    .text_color(rgb(status_fg))
                    .child(status_label),
            )
            .child(div().flex_1().child(path))
    }

    fn render_diff_and_threads(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let lines = parse_diff(&self.state.diff_text);
        let highlighter = Highlighter::global();
        let lang = self
            .state
            .selected_path()
            .and_then(lang_for_path);

        let mut children = Vec::<gpui::AnyElement>::with_capacity(lines.len());
        for (i, line) in lines.iter().enumerate() {
            children.push(self.render_diff_line(i, line, lang, highlighter, cx).into_any_element());
            // Inline thread panels under their anchor line.
            if line.new_line > 0 {
                for t in self.state.threads_for_line(line.new_line) {
                    children
                        .push(self.render_thread_card(t, cx, true).into_any_element());
                }
            }
        }

        let header_label = self.file_name();
        div()
            .flex()
            .flex_col()
            .flex_grow(1.0)
            .h_full()
            .min_w_0()
            .child(
                div()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .text_color(rgb(MUTED))
                    .child(header_label),
            )
            .child(
                div()
                    .id("diff-scroller")
                    .flex_grow(1.0)
                    .flex()
                    .flex_col()
                    .overflow_y_scroll()
                    .children(children),
            )
    }

    fn render_diff_line(
        &self,
        row: usize,
        line: &DiffLine,
        lang: Option<&str>,
        highlighter: &Highlighter,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let (bg, default_fg) = match line.kind {
            DiffLineKind::Added => (Some(ADD_BG), ADD_FG),
            DiffLineKind::Removed => (Some(REM_BG), REM_FG),
            DiffLineKind::Hunk => (Some(HUNK_BG), ACCENT),
            DiffLineKind::Header => (None, MUTED),
            DiffLineKind::Context => (None, FG),
        };

        let line_number = if line.new_line > 0 {
            SharedString::from(line.new_line.to_string())
        } else {
            SharedString::from("")
        };
        let marker = match line.kind {
            DiffLineKind::Added => "+",
            DiffLineKind::Removed => "-",
            _ => " ",
        };

        // syntect only runs on the code payload (sans +/- prefix); header and
        // hunk lines render as a single colored run.
        let code_body = render_line_content(line, lang, highlighter, default_fg);

        let anchorable = matches!(line.kind, DiffLineKind::Added | DiffLineKind::Context)
            && line.new_line > 0;
        let anchor_line = line.new_line;

        let add_btn = div()
            .id(("add", row))
            .flex_shrink_0()
            .w(px(16.))
            .h(px(16.))
            .items_center()
            .justify_center()
            .text_size(px(11.))
            .text_color(rgb(MUTED))
            .cursor_pointer()
            .when(anchorable, |d| {
                d.hover(|d| d.bg(rgb(ACCENT)).text_color(rgb(0xffffff)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.copy_new_thread_hint(anchor_line, cx);
                    }))
                    .child("+")
            });

        let mut row_el = div()
            .flex()
            .flex_row()
            .items_start()
            .text_color(rgb(default_fg))
            .child(
                div()
                    .flex_shrink_0()
                    .w(px(48.))
                    .px_2()
                    .text_color(rgb(MUTED))
                    .child(line_number),
            )
            .child(div().flex_shrink_0().w(px(10.)).child(marker))
            .child(add_btn)
            .child(div().flex_grow(1.0).child(code_body));
        if let Some(bg_rgb) = bg {
            row_el = row_el.bg(rgb(bg_rgb));
        }
        row_el
    }

    fn render_thread_card(
        &self,
        thread: &Thread,
        cx: &mut Context<Self>,
        inline: bool,
    ) -> impl IntoElement + use<> {
        let state = if thread.resolved { "resolved" } else { "open" };
        let state_color = if thread.resolved { MUTED } else { ACCENT };
        let header_text = SharedString::from(format!(
            "Thread #{} · {}{}  · {}",
            thread.id,
            thread.file_path,
            if thread.line > 0 {
                format!(":{}", thread.line)
            } else {
                "".into()
            },
            state,
        ));

        let mut comment_els = Vec::<gpui::AnyElement>::with_capacity(thread.comments.len());
        for c in &thread.comments {
            comment_els.push(render_comment(c).into_any_element());
        }

        let thread_id = thread.id;
        let resolved = thread.resolved;
        let resolve_btn = div()
            .id(("resolve", thread_id as usize))
            .px_2()
            .py_1()
            .rounded(px(4.))
            .border_1()
            .border_color(rgb(BORDER))
            .text_size(px(10.))
            .text_color(rgb(MUTED))
            .cursor_pointer()
            .when(!resolved, |d| {
                d.hover(|d| d.text_color(rgb(FG)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.resolve_thread(thread_id, cx);
                    }))
                    .child("Resolve")
            })
            .when(resolved, |d| d.child("Resolved"));

        let reply_btn = div()
            .id(("reply", thread_id as usize))
            .px_2()
            .py_1()
            .rounded(px(4.))
            .border_1()
            .border_color(rgb(BORDER))
            .text_size(px(10.))
            .text_color(rgb(MUTED))
            .cursor_pointer()
            .hover(|d| d.text_color(rgb(FG)))
            .on_click(cx.listener(move |this, _, _, cx| this.copy_reply_hint(thread_id, cx)))
            .child("Copy reply cmd");

        let mut card = div()
            .flex()
            .flex_col()
            .gap_1()
            .p_2()
            .mb_1()
            .bg(rgb(0x121520))
            .border_1()
            .border_color(rgb(BORDER))
            .rounded(px(6.))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .text_size(px(11.))
                    .text_color(rgb(state_color))
                    .child(header_text)
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_2()
                            .child(reply_btn)
                            .child(resolve_btn),
                    ),
            )
            .child(div().flex().flex_col().gap_1().children(comment_els));
        if inline {
            card = card.ml(px(70.)).mr(px(12.)).my_1();
        }
        card
    }

    fn render_threads_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut cards = Vec::<gpui::AnyElement>::with_capacity(self.state.threads.len());
        for t in &self.state.threads {
            cards.push(self.render_thread_card(t, cx, false).into_any_element());
        }
        let count = self.state.threads.len();

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

// -------- Standalone render helpers --------

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

fn status_color(status: &str) -> u32 {
    match status {
        "added" | "untracked" => 0x40c057,
        "modified" => ACCENT,
        "deleted" => 0xe03131,
        _ => MUTED,
    }
}

fn render_line_content(
    line: &DiffLine,
    lang: Option<&str>,
    highlighter: &Highlighter,
    default_fg: u32,
) -> impl IntoElement {
    // Hunk + header render as a single uncolored span.
    if matches!(line.kind, DiffLineKind::Hunk | DiffLineKind::Header) {
        return div().child(SharedString::from(line.text.clone())).into_any_element();
    }

    let body = match line.kind {
        DiffLineKind::Added | DiffLineKind::Removed => &line.text[1..],
        _ => line.text.as_str(),
    };

    let maybe_spans = highlighter.highlight_line(lang, body);
    match maybe_spans {
        Some(spans) if !spans.is_empty() => {
            let runs: Vec<gpui::AnyElement> = spans
                .into_iter()
                .map(|(rgb_color, text)| {
                    div()
                        .text_color(rgb(rgb_color))
                        .child(SharedString::from(text))
                        .into_any_element()
                })
                .collect();
            div()
                .flex()
                .flex_row()
                .children(runs)
                .into_any_element()
        }
        _ => div()
            .text_color(rgb(default_fg))
            .child(SharedString::from(body.to_string()))
            .into_any_element(),
    }
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
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .text_size(px(10.))
                .text_color(rgb(kind_color))
                .child(header),
        )
        .child(div().text_color(rgb(FG)).child(body))
}

fn shell_escape(s: &str) -> String {
    if s.chars().all(|c| c.is_ascii_alphanumeric() || "-_./:".contains(c)) {
        s.to_owned()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}
