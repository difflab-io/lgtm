//! `DiffView` — the middle pane as its own GPUI entity.
//!
//! Splitting the diff out of `ReviewView` means layout / render invalidation
//! for the Textarea composer (which happens on every keystroke) stays inside
//! this entity. Mouse hover on a file-list item or an unrelated top-level
//! element doesn't re-render the 200-row diff tree, and vice-versa.
//!
//! The parent (`ReviewView`) talks to this view through plain method calls on
//! `Entity<DiffView>::update`. The view owns everything about the currently
//! selected file: diff text, cached syntect spans, threads, and the composer.

use std::sync::Arc;

use anyhow::Result;
use gpui_kit::component::input::{Enter, Escape, Textarea, TextareaState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::client::LgtmClient;
use crate::diff::{DiffLineKind, parse_diff};
use crate::highlight::{Highlighter, lang_for_path};
use crate::proto::{AuthorKind, Comment, Thread, User};
use crate::review_state::{CachedDiffLine, ComposerTarget, ReviewConfig};

// ---- Palette tokens (mirror of src/review.rs). ----
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
const CARD_BG: u32 = 0x121520;
const AGENT: u32 = 0xf59f00;

pub struct DiffView {
    config: ReviewConfig,
    client: Arc<LgtmClient>,
    user: Option<User>,
    selected: Option<String>,
    diff_lines: Vec<CachedDiffLine>,
    threads: Vec<Thread>,
    composer: Option<Composer>,
    loading: bool,
    error: Option<String>,
}

struct Composer {
    target: ComposerTarget,
    input: Entity<TextareaState>,
}

/// Snapshot requested by `ReviewView` to render the threads sidebar + file
/// badges without observing `DiffView`'s per-keystroke updates.
pub struct ThreadsSnapshot {
    pub selected: Option<String>,
    pub threads: Vec<Thread>,
}

enum ComposerSubmit {
    NewThread(Thread),
    Reply(i64, Comment),
}

impl DiffView {
    pub fn new(config: ReviewConfig, client: Arc<LgtmClient>) -> Self {
        Self {
            config,
            client,
            user: None,
            selected: None,
            diff_lines: Vec::new(),
            threads: Vec::new(),
            composer: None,
            loading: false,
            error: None,
        }
    }

    pub fn set_user(&mut self, user: User, cx: &mut Context<Self>) {
        self.user = Some(user);
        cx.notify();
    }

    pub fn snapshot(&self) -> ThreadsSnapshot {
        ThreadsSnapshot {
            selected: self.selected.clone(),
            threads: self.threads.clone(),
        }
    }

    pub fn selected(&self) -> Option<&str> {
        self.selected.as_deref()
    }

    pub fn set_selected_file(&mut self, path: String, cx: &mut Context<Self>) {
        self.selected = Some(path.clone());
        self.diff_lines.clear();
        self.threads.clear();
        self.composer = None;
        self.loading = true;
        self.error = None;
        cx.notify();
        // Debug-only: auto-open a composer right after the diff loads so we
        // can benchmark keystroke-driven re-renders without having to click
        // "+" manually. Set LGTM_AUTO_COMPOSER=1 to enable.
        if std::env::var("LGTM_AUTO_COMPOSER").is_ok() {
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(400))
                    .await;
                let _ = this.update(cx, |_this, _cx| {
                    // Can't open a composer without a Window; just notify.
                });
            })
            .detach();
        }

        let client = self.client.clone();
        let repo = self.config.repo_path.clone();
        let base = self.config.base_ref.clone();
        cx.spawn(async move |this, cx| {
            let result: Result<(Vec<CachedDiffLine>, Vec<Thread>)> = cx
                .background_spawn(async move {
                    let diff = client.get_file_diff(&repo, &base, &path)?;
                    let threads = client.list_threads(&repo, &base, Some(&path))?;
                    let lang = lang_for_path(&path);
                    let highlighter = Highlighter::global();
                    let cached: Vec<CachedDiffLine> = parse_diff(&diff)
                        .into_iter()
                        .map(|l| {
                            let spans = match l.kind {
                                DiffLineKind::Hunk | DiffLineKind::Header => None,
                                _ => {
                                    let body = match l.kind {
                                        DiffLineKind::Added | DiffLineKind::Removed => {
                                            &l.text[1..]
                                        }
                                        _ => l.text.as_str(),
                                    };
                                    highlighter.highlight_line(lang, body)
                                }
                            };
                            CachedDiffLine {
                                kind: l.kind,
                                text: l.text,
                                new_line: l.new_line,
                                spans,
                            }
                        })
                        .collect();
                    Ok((cached, threads))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.loading = false;
                match result {
                    Ok((lines, threads)) => {
                        this.diff_lines = lines;
                        this.threads = threads;
                    }
                    Err(e) => this.error = Some(format!("{e:#}")),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn resolve_thread(&mut self, thread_id: i64, cx: &mut Context<Self>) {
        let client = self.client.clone();
        cx.spawn(async move |this, cx| {
            let result: Result<Thread> = cx
                .background_spawn(async move { client.resolve_thread(thread_id) })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(updated) => {
                        for t in this.threads.iter_mut() {
                            if t.id == updated.id {
                                *t = updated.clone();
                            }
                        }
                    }
                    Err(e) => this.error = Some(format!("{e:#}")),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn start_composer(
        &mut self,
        target: ComposerTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let placeholder = match target {
            ComposerTarget::NewThread { .. } => "Leave a comment (Cmd+Enter or Submit to post)",
            ComposerTarget::Reply { .. } => "Reply (Cmd+Enter or Submit to post)",
        };
        let input = cx.new(|cx| TextareaState::new(window, cx).placeholder(placeholder));
        let handle = input.read(cx).focus_handle(cx);
        window.focus(&handle, cx);
        self.composer = Some(Composer { target, input });
        cx.notify();
    }

    fn cancel_composer(&mut self, cx: &mut Context<Self>) {
        self.composer = None;
        cx.notify();
    }

    fn submit_composer(&mut self, cx: &mut Context<Self>) {
        let Some(composer) = self.composer.take() else { return };
        let body = composer.input.read(cx).value().to_string().trim().to_owned();
        if body.is_empty() {
            self.composer = Some(composer);
            return;
        }
        let Some(user) = self.user.clone() else {
            self.error = Some("identity not yet resolved".into());
            self.composer = Some(composer);
            cx.notify();
            return;
        };

        let client = self.client.clone();
        let repo = self.config.repo_path.clone();
        let base = self.config.base_ref.clone();
        let selected = self.selected.clone();
        let target = composer.target.clone();

        cx.spawn(async move |this, cx| {
            let result: Result<ComposerSubmit> = cx
                .background_spawn(async move {
                    match target {
                        ComposerTarget::NewThread { line } => {
                            let file = selected
                                .ok_or_else(|| anyhow::anyhow!("no file selected"))?;
                            let thread = client
                                .create_thread(&repo, &base, &file, line, user.id, &body)?;
                            Ok(ComposerSubmit::NewThread(thread))
                        }
                        ComposerTarget::Reply { thread_id } => {
                            let comment =
                                client.append_comment(thread_id, user.id, &body)?;
                            Ok(ComposerSubmit::Reply(thread_id, comment))
                        }
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(ComposerSubmit::NewThread(t)) => this.threads.push(t),
                    Ok(ComposerSubmit::Reply(tid, c)) => {
                        for t in this.threads.iter_mut() {
                            if t.id == tid {
                                t.comments.push(c.clone());
                            }
                        }
                    }
                    Err(e) => this.error = Some(format!("{e:#}")),
                }
                cx.notify();
            });
        })
        .detach();
    }
}

impl Render for DiffView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if std::env::var("LGTM_TRACE_RENDER").is_ok() {
            use std::sync::atomic::{AtomicU64, Ordering};
            static N: AtomicU64 = AtomicU64::new(0);
            let n = N.fetch_add(1, Ordering::Relaxed) + 1;
            eprintln!(
                "[lgtm-gpui] DiffView render #{n} lines={} composer={}",
                self.diff_lines.len(),
                self.composer.is_some()
            );
        }
        // Debug: auto-open a composer anchored at line 1 of the current file
        // on first render after the diff loads. Lets us benchmark typing lag
        // without manually clicking "+". Enable with LGTM_AUTO_COMPOSER=1.
        if std::env::var("LGTM_AUTO_COMPOSER").is_ok()
            && self.composer.is_none()
            && !self.diff_lines.is_empty()
        {
            self.start_composer(ComposerTarget::NewThread { line: 1 }, window, cx);
        }

        let header_label = self
            .selected
            .clone()
            .map(SharedString::from)
            .unwrap_or_else(|| "Select a file".into());

        let mut children = Vec::<AnyElement>::with_capacity(self.diff_lines.len());
        for (i, line) in self.diff_lines.iter().enumerate() {
            children.push(self.render_diff_line(i, line, cx).into_any_element());

            if let Some(c) = &self.composer {
                if matches!(c.target, ComposerTarget::NewThread { line: a } if a == line.new_line)
                    && line.new_line > 0
                {
                    children.push(self.render_composer_inline(c, cx).into_any_element());
                }
            }

            if line.new_line > 0 {
                let threads: Vec<&Thread> = self
                    .threads
                    .iter()
                    .filter(|t| t.line == line.new_line)
                    .collect();
                for t in threads {
                    children.push(self.render_thread_card(t, cx).into_any_element());
                    if let Some(c) = &self.composer {
                        if c.target == (ComposerTarget::Reply { thread_id: t.id }) {
                            children
                                .push(self.render_composer_inline(c, cx).into_any_element());
                        }
                    }
                }
            }
        }

        let error_banner = self.error.clone().map(|msg| {
            div()
                .px_3()
                .py_2()
                .bg(rgb(0x2a0f12))
                .text_color(rgb(REM_FG))
                .child(SharedString::from(msg))
        });

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
            .children(error_banner)
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
}

impl DiffView {
    fn render_diff_line(
        &self,
        row: usize,
        line: &CachedDiffLine,
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

        let code_body = render_line_content(line, default_fg);
        let anchorable = matches!(line.kind, DiffLineKind::Added | DiffLineKind::Context)
            && line.new_line > 0;
        let anchor_line = line.new_line;

        let add_btn_slot = if anchorable {
            div()
                .id(("add", row))
                .flex_shrink_0()
                .w(px(16.))
                .h(px(16.))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(11.))
                .text_color(rgb(MUTED))
                .cursor_pointer()
                .hover(|d| d.bg(rgb(ACCENT)).text_color(rgb(0xffffff)))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.start_composer(
                        ComposerTarget::NewThread { line: anchor_line },
                        window,
                        cx,
                    );
                }))
                .child("+")
                .into_any_element()
        } else {
            div().flex_shrink_0().w(px(16.)).h(px(16.)).into_any_element()
        };

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
            .child(add_btn_slot)
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
    ) -> impl IntoElement + use<> {
        let state = if thread.resolved { "resolved" } else { "open" };
        let state_color = if thread.resolved { MUTED } else { ACCENT };
        let header_text = SharedString::from(format!(
            "Thread #{} · {}{}  · {}",
            thread.id,
            thread.file_path,
            if thread.line > 0 { format!(":{}", thread.line) } else { "".into() },
            state,
        ));

        let mut comment_els = Vec::<AnyElement>::with_capacity(thread.comments.len());
        for c in &thread.comments {
            comment_els.push(render_comment(c).into_any_element());
        }

        let thread_id = thread.id;
        let resolved = thread.resolved;

        let resolve_btn = if resolved {
            div()
                .px_2()
                .py_1()
                .rounded(px(4.))
                .border_1()
                .border_color(rgb(BORDER))
                .text_size(px(10.))
                .text_color(rgb(MUTED))
                .child("Resolved")
                .into_any_element()
        } else {
            div()
                .id(("resolve", thread_id as usize))
                .px_2()
                .py_1()
                .rounded(px(4.))
                .border_1()
                .border_color(rgb(BORDER))
                .text_size(px(10.))
                .text_color(rgb(MUTED))
                .cursor_pointer()
                .hover(|d| d.text_color(rgb(FG)))
                .on_click(
                    cx.listener(move |this, _, _, cx| this.resolve_thread(thread_id, cx)),
                )
                .child("Resolve")
                .into_any_element()
        };

        let reply_btn = if resolved {
            div().into_any_element()
        } else {
            div()
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
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.start_composer(ComposerTarget::Reply { thread_id }, window, cx);
                }))
                .child("Reply")
                .into_any_element()
        };

        div()
            .flex()
            .flex_col()
            .gap_1()
            .p_2()
            .my_1()
            .ml(px(70.))
            .mr(px(12.))
            .bg(rgb(CARD_BG))
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
            .child(div().flex().flex_col().gap_1().children(comment_els))
    }

    fn render_composer_inline(
        &self,
        composer: &Composer,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let header = SharedString::from(match composer.target {
            ComposerTarget::NewThread { line } => format!("New thread @ line {line}"),
            ComposerTarget::Reply { thread_id } => format!("Reply to #{thread_id}"),
        });

        div()
            .ml(px(70.))
            .mr(px(12.))
            .my_1()
            .flex()
            .flex_col()
            .gap_1()
            .p_2()
            .bg(rgb(CARD_BG))
            .border_1()
            .border_color(rgb(ACCENT))
            .rounded(px(6.))
            .on_action(cx.listener(|this, action: &Enter, _window, cx| {
                if action.secondary {
                    this.submit_composer(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Escape, _window, cx| {
                this.cancel_composer(cx);
            }))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .text_size(px(11.))
                    .text_color(rgb(ACCENT))
                    .child(header)
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_2()
                            .child(
                                div()
                                    .id("composer-cancel")
                                    .px_2()
                                    .py_1()
                                    .rounded(px(4.))
                                    .border_1()
                                    .border_color(rgb(BORDER))
                                    .text_size(px(10.))
                                    .text_color(rgb(MUTED))
                                    .cursor_pointer()
                                    .hover(|d| d.text_color(rgb(FG)))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.cancel_composer(cx)
                                    }))
                                    .child("Cancel"),
                            )
                            .child(
                                div()
                                    .id("composer-submit")
                                    .px_2()
                                    .py_1()
                                    .rounded(px(4.))
                                    .bg(rgb(ACCENT))
                                    .text_size(px(10.))
                                    .text_color(rgb(0xffffff))
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.submit_composer(cx)
                                    }))
                                    .child("Submit"),
                            ),
                    ),
            )
            .child(Textarea::new(&composer.input))
            .child(
                div()
                    .text_size(px(10.))
                    .text_color(rgb(MUTED))
                    .child("Cmd+Enter: submit · Esc: cancel · Enter: newline · Click Submit to post"),
            )
    }
}

// -------- Standalone render helpers (local to DiffView) --------

fn render_line_content(line: &CachedDiffLine, default_fg: u32) -> impl IntoElement {
    if matches!(line.kind, DiffLineKind::Hunk | DiffLineKind::Header) {
        return div().child(SharedString::from(line.text.clone())).into_any_element();
    }
    let body = match line.kind {
        DiffLineKind::Added | DiffLineKind::Removed => &line.text[1..],
        _ => line.text.as_str(),
    };
    match &line.spans {
        Some(spans) if !spans.is_empty() => {
            let runs: Vec<AnyElement> = spans
                .iter()
                .map(|(rgb_color, text)| {
                    div()
                        .text_color(rgb(*rgb_color))
                        .child(SharedString::from(text.clone()))
                        .into_any_element()
                })
                .collect();
            div().flex().flex_row().children(runs).into_any_element()
        }
        _ => div()
            .text_color(rgb(default_fg))
            .child(SharedString::from(body.to_string()))
            .into_any_element(),
    }
}

fn render_comment(c: &Comment) -> impl IntoElement {
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
