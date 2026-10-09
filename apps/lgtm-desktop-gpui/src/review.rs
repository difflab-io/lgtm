//! Three-pane review view on top of gpui-kit.
//!
//! Interaction:
//! - Clicking a changed file loads its diff + threads.
//! - Hover a line to reveal "+"; click to open an inline composer anchored
//!   at that line. The composer is a `gpui_kit::component::input::Textarea`,
//!   so arrow keys, selection, backspace, clipboard, and IME all work.
//! - `PressEnter` (plain Enter) submits via `CreateThread` or
//!   `AppendComment`; Shift+Enter inserts a newline. "Cancel" closes it.
//! - Each thread has a `Reply` button (opens a scoped composer) and a
//!   `Resolve` button (direct API call).

use std::sync::Arc;

use anyhow::Result;
use gpui_kit::component::input::{Enter, Escape, Textarea, TextareaState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::client::LgtmClient;
use crate::diff::{DiffLineKind, parse_diff};
use crate::highlight::{Highlighter, lang_for_path};
use crate::proto::{AuthorKind, ChangedFile, Comment, Thread, User};
use crate::review_state::{CachedDiffLine, ComposerTarget, ReviewConfig, ReviewState};

// ---- Colour tokens (match the SPA palette). ----
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

pub struct ReviewView {
    config: ReviewConfig,
    client: Arc<LgtmClient>,
    state: ReviewState,
    /// At most one composer is open at a time. Dropping it (via
    /// `cancel_composer` / `submit_composer`) also drops its subscription.
    composer: Option<Composer>,
}

struct Composer {
    target: ComposerTarget,
    input: Entity<TextareaState>,
}

impl ReviewView {
    pub fn new(config: ReviewConfig, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let client = Arc::new(LgtmClient::new(config.backend.clone()));
        let mut this = Self {
            config: config.clone(),
            client: client.clone(),
            state: ReviewState::default(),
            composer: None,
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
        self.state.diff_lines.clear();
        self.state.threads.clear();
        self.composer = None;
        self.state.loading = true;
        cx.notify();

        let client = self.client.clone();
        let repo = self.config.repo_path.clone();
        let base = self.config.base_ref.clone();
        let path_for_fetch = path.clone();
        cx.spawn(async move |this, cx| {
            // Fetch diff + threads, then pre-highlight every diff line on
            // the background executor so interactive re-renders (keystrokes
            // in the composer, thread resolve, etc.) don't repay the
            // syntect cost.
            let result: Result<(String, Vec<CachedDiffLine>, Vec<Thread>)> = cx
                .background_spawn(async move {
                    let diff = client.get_file_diff(&repo, &base, &path_for_fetch)?;
                    let threads = client.list_threads(&repo, &base, Some(&path_for_fetch))?;
                    let lang = lang_for_path(&path_for_fetch);
                    let highlighter = Highlighter::global();
                    let cached = parse_diff(&diff)
                        .into_iter()
                        .map(|l| {
                            let spans = match l.kind {
                                DiffLineKind::Hunk | DiffLineKind::Header => None,
                                _ => {
                                    let body = match l.kind {
                                        DiffLineKind::Added | DiffLineKind::Removed => &l.text[1..],
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
                    Ok((diff, cached, threads))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.state.loading = false;
                match result {
                    Ok((diff, cached, threads)) => {
                        this.state.diff_text = diff;
                        this.state.diff_lines = cached;
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

    // -------- Composer lifecycle --------

    fn start_composer(&mut self, target: ComposerTarget, window: &mut Window, cx: &mut Context<Self>) {
        // Deliberately NOT subscribing to the TextareaState entity: in gpui-kit,
        // `cx.subscribe_in` on an entity causes the subscribing view to re-render
        // on every emitted event — including per-keystroke `InputEvent::Change`.
        // On a diff with 200+ lines, that re-paints the entire tree on every
        // keypress and causes visible input lag. Submit is driven instead by an
        // explicit "Submit" button + Cmd+Enter keybind (see render_composer_inline).
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
        let body = composer.input.read(cx).value().to_string();
        let body = body.trim().to_owned();
        if body.is_empty() {
            // Nothing to submit — put the composer back.
            self.composer = Some(composer);
            return;
        }
        let Some(user) = self.state.user.clone() else {
            self.state.error = Some("identity not yet resolved".into());
            self.composer = Some(composer);
            cx.notify();
            return;
        };

        let client = self.client.clone();
        let repo = self.config.repo_path.clone();
        let base = self.config.base_ref.clone();
        let selected = self.state.selected.clone();
        let target = composer.target.clone();

        cx.spawn(async move |this, cx| {
            let result: Result<ComposerSubmit> = cx
                .background_spawn(async move {
                    match target {
                        ComposerTarget::NewThread { line } => {
                            let file = selected.ok_or_else(|| anyhow::anyhow!("no file selected"))?;
                            let thread = client.create_thread(&repo, &base, &file, line, user.id, &body)?;
                            Ok(ComposerSubmit::NewThread(thread))
                        }
                        ComposerTarget::Reply { thread_id } => {
                            let comment = client.append_comment(thread_id, user.id, &body)?;
                            Ok(ComposerSubmit::Reply(thread_id, comment))
                        }
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(ComposerSubmit::NewThread(t)) => this.state.threads.push(t),
                    Ok(ComposerSubmit::Reply(tid, c)) => {
                        for t in this.state.threads.iter_mut() {
                            if t.id == tid {
                                t.comments.push(c.clone());
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

    fn file_name(&self) -> SharedString {
        self.state
            .selected_path()
            .map(|p| SharedString::from(p.to_string()))
            .unwrap_or_else(|| "Select a file".into())
    }
}

enum ComposerSubmit {
    NewThread(Thread),
    Reply(i64, Comment),
}

impl Render for ReviewView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if std::env::var("LGTM_TRACE_RENDER").is_ok() {
            use std::sync::atomic::{AtomicU64, Ordering};
            static N: AtomicU64 = AtomicU64::new(0);
            let n = N.fetch_add(1, Ordering::Relaxed) + 1;
            eprintln!(
                "[lgtm-gpui] render #{n} lines={} composer={}",
                self.state.diff_lines.len(),
                self.composer.is_some()
            );
        }
        div()
            .flex()
            .flex_col()
            .w_full()
            .h_full()
            .bg(rgb(BG))
            .text_color(rgb(FG))
            .text_size(px(13.0))
            // Reserve the macOS titlebar strip (28 px) so the traffic lights
            // don't overlap the sidebar / column headers.
            .child(div().h(px(28.)).flex_shrink_0())
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_grow(1.0)
                    .min_h(px(0.))
                    .child(self.render_sidebar(cx))
                    .child(self.render_diff_and_threads(cx))
                    .child(self.render_threads_panel(cx)),
            )
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
        let lines = &self.state.diff_lines;

        let mut children = Vec::<AnyElement>::with_capacity(lines.len());
        for (i, line) in lines.iter().enumerate() {
            children.push(self.render_diff_line(i, line, cx).into_any_element());

            // Inline composer anchored at this new-file line.
            if let Some(c) = &self.composer {
                if matches!(c.target, ComposerTarget::NewThread { line: anchor } if anchor == line.new_line)
                    && line.new_line > 0
                {
                    children.push(self.render_composer_inline(c, cx).into_any_element());
                }
            }

            // Threads at this line + optional reply composer under each.
            if line.new_line > 0 {
                for t in self.state.threads_for_line(line.new_line) {
                    children.push(self.render_thread_card(t, cx, true).into_any_element());
                    if let Some(c) = &self.composer {
                        if c.target == (ComposerTarget::Reply { thread_id: t.id }) {
                            children.push(self.render_composer_inline(c, cx).into_any_element());
                        }
                    }
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
        inline: bool,
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
                .on_click(cx.listener(move |this, _, _, cx| this.resolve_thread(thread_id, cx)))
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

        let mut card = div()
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
        let mut cards = Vec::<AnyElement>::with_capacity(self.state.threads.len());
        for t in &self.state.threads {
            cards.push(self.render_thread_card(t, cx, false).into_any_element());
            if let Some(c) = &self.composer {
                if c.target == (ComposerTarget::Reply { thread_id: t.id }) {
                    cards.push(self.render_composer_inline(c, cx).into_any_element());
                }
            }
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

    fn render_composer_inline(
        &self,
        composer: &Composer,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let header = SharedString::from(match composer.target {
            ComposerTarget::NewThread { line } => format!("New thread @ line {}", line),
            ComposerTarget::Reply { thread_id } => format!("Reply to #{}", thread_id),
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
            // Scope Cmd+Enter / Escape to this composer. The TextareaState's
            // own key bindings still handle plain Enter (newline), typing,
            // arrows, etc. — the parent only intercepts the two shortcuts.
            // Textarea itself dispatches `Enter` (plain / shift / secondary)
            // and `Escape` actions via its own key bindings; we listen as the
            // actions bubble up through this composer div. Secondary-enter
            // (Cmd+Enter on macOS) → submit. Plain Enter is left for Textarea
            // to insert a newline. Escape → cancel.
            //
            // This avoids `cx.subscribe_in` on the Textarea entity, which
            // caused per-keystroke parent re-renders (visible as composer
            // input lag).
            .on_action(cx.listener(|this, action: &Enter, _window, cx| {
                if action.secondary {
                    this.submit_composer(cx);
                }
            }))
            .on_action(cx.listener(|this, _action: &Escape, _window, cx| {
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

fn render_line_content(line: &CachedDiffLine, default_fg: u32) -> impl IntoElement {
    // Hunk + header render as a single uncolored span.
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
