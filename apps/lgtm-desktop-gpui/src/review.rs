//! Three-pane review view: file list (left), diff + inline threads (middle),
//! threads sidebar (right). Mirrors the SPA layout and class semantics.
//!
//! Interaction:
//! - Clicking a changed file loads its diff + threads from the server.
//! - Hovering a diff line reveals a "+" button; clicking opens the inline
//!   composer at that line. Submitting with Enter calls `CreateThread`.
//! - Each thread has a `Reply` button (opens the composer below the thread,
//!   submits via `AppendComment`) and a `Resolve` button (direct API call).
//!
//! Text input is hand-rolled on top of GPUI's focus + key-down primitives.
//! It supports: typing, backspace, Enter (submit), Shift+Enter (newline),
//! Escape (cancel). No cursor movement, selection, or clipboard — this is
//! intentional for the PoC; a follow-up can swap in Zed's `editor` crate.

use std::sync::Arc;

use anyhow::Result;
use gpui::{
    App, AppContext, Context, FocusHandle, FontWeight, InteractiveElement, IntoElement,
    KeyDownEvent, ParentElement, Render, SharedString, StatefulInteractiveElement, Styled, Window,
    div, prelude::FluentBuilder, px, rgb,
};

use crate::client::LgtmClient;
use crate::diff::{DiffLine, DiffLineKind, parse_diff};
use crate::highlight::{Highlighter, lang_for_path};
use crate::proto::{AuthorKind, ChangedFile, Comment, Thread, User};
use crate::review_state::{Composer, ComposerTarget, ReviewConfig, ReviewState};

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
    composer_focus: FocusHandle,
    /// Toggles every ~530 ms to drive the composer cursor blink.
    cursor_on: bool,
}

impl ReviewView {
    pub fn new(config: ReviewConfig, cx: &mut Context<Self>) -> Self {
        let client = Arc::new(LgtmClient::new(config.backend.clone()));
        let mut this = Self {
            config: config.clone(),
            client: client.clone(),
            state: ReviewState::default(),
            composer_focus: cx.focus_handle(),
            cursor_on: true,
        };
        this.spawn_initial_load(cx);
        this.spawn_cursor_blink(cx);
        this
    }

    /// Background timer that flips `cursor_on` every 530 ms and triggers
    /// a re-render — but only when there's a composer open, so idle views
    /// don't repaint. 530 ms matches macOS's default caret blink period.
    fn spawn_cursor_blink(&mut self, cx: &mut Context<Self>) {
        use std::time::Duration;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(530))
                    .await;
                let keep_going = this
                    .update(cx, |this, cx| {
                        if this.state.composer.is_some() {
                            this.cursor_on = !this.cursor_on;
                            cx.notify();
                        } else {
                            // Reset to "on" so the first keystroke after
                            // opening the composer shows a stable caret.
                            this.cursor_on = true;
                        }
                        true
                    })
                    .is_ok();
                if !keep_going {
                    break;
                }
            }
        })
        .detach();
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
        self.state.composer = None;
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

    // -------- Composer lifecycle --------

    fn start_composer(&mut self, target: ComposerTarget, window: &mut Window, cx: &mut Context<Self>) {
        self.state.composer = Some(Composer { target, body: String::new() });
        window.focus(&self.composer_focus, cx);
        cx.notify();
    }

    fn cancel_composer(&mut self, cx: &mut Context<Self>) {
        self.state.composer = None;
        cx.notify();
    }

    fn submit_composer(&mut self, cx: &mut Context<Self>) {
        let Some(composer) = self.state.composer.take() else { return };
        let body = composer.body.trim().to_owned();
        if body.is_empty() {
            // Put the composer back if the user hits Enter on an empty body.
            self.state.composer = Some(composer);
            return;
        }
        let Some(user) = self.state.user.clone() else {
            self.state.error = Some("identity not yet resolved".into());
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

    fn handle_composer_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let ks = &event.keystroke;
        // Submit: plain Enter. Shift+Enter inserts a newline.
        if ks.key == "enter" {
            if ks.modifiers.shift {
                if let Some(c) = self.state.composer.as_mut() {
                    c.body.push('\n');
                    cx.notify();
                }
            } else {
                self.submit_composer(cx);
            }
            return;
        }
        if ks.key == "escape" {
            self.cancel_composer(cx);
            return;
        }
        if ks.key == "backspace" {
            if let Some(c) = self.state.composer.as_mut() {
                c.body.pop();
                cx.notify();
            }
            return;
        }
        // Ignore shortcut keystrokes (cmd+anything, ctrl+anything).
        if ks.modifiers.platform || ks.modifiers.control {
            return;
        }
        // Insert the typed character — key_char is the final composed char
        // (handles shift / option-dead-keys / IMEs).
        if let Some(ch) = &ks.key_char {
            if let Some(c) = self.state.composer.as_mut() {
                c.body.push_str(ch);
                cx.notify();
            }
        }
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
        // Reserve the macOS titlebar strip (28 px) so the traffic lights don't
        // overlap the sidebar/column headers. The titlebar itself is drawn by
        // the system; this is just empty same-bg space under it.
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
        let lines = parse_diff(&self.state.diff_text);
        let highlighter = Highlighter::global();
        let lang = self.state.selected_path().and_then(lang_for_path);

        let mut children = Vec::<gpui::AnyElement>::with_capacity(lines.len());
        for (i, line) in lines.iter().enumerate() {
            children.push(self.render_diff_line(i, line, lang, highlighter, cx).into_any_element());

            // Inline composer takes priority at its anchor line.
            let composer_here = matches!(
                &self.state.composer,
                Some(c) if matches!(c.target, ComposerTarget::NewThread { line } if line == line_from_i32(line_from_i32(l_new(line))) )
            );
            // Simpler to just test directly:
            if let Some(c) = &self.state.composer {
                if let ComposerTarget::NewThread { line: anchor } = c.target {
                    if anchor == line.new_line && line.new_line > 0 {
                        children.push(self.render_composer_inline(c, cx).into_any_element());
                    }
                }
            }

            // Threads at this new-file line, each with its own card (and
            // optionally a reply composer below it).
            if line.new_line > 0 {
                for t in self.state.threads_for_line(line.new_line) {
                    children.push(self.render_thread_card(t, cx, true).into_any_element());
                    if let Some(c) = &self.state.composer {
                        if c.target == (ComposerTarget::Reply { thread_id: t.id }) {
                            children.push(self.render_composer_inline(c, cx).into_any_element());
                        }
                    }
                }
            }

            let _ = composer_here; // silence
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

        let code_body = render_line_content(line, lang, highlighter, default_fg);

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

        let mut comment_els = Vec::<gpui::AnyElement>::with_capacity(thread.comments.len());
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
        let mut cards = Vec::<gpui::AnyElement>::with_capacity(self.state.threads.len());
        for t in &self.state.threads {
            cards.push(self.render_thread_card(t, cx, false).into_any_element());
            if let Some(c) = &self.state.composer {
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
        let caret = if self.cursor_on { "▎" } else { " " };
        let body = SharedString::from(if composer.body.is_empty() {
            // Prefix the placeholder with the caret so the user sees focus even
            // before they start typing.
            format!("{caret}{}", composer.placeholder())
        } else {
            // Append a blinking caret at the end of the typed text.
            format!("{}{caret}", composer.body)
        });
        let is_placeholder = composer.body.is_empty();
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
            .track_focus(&self.composer_focus)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _window, cx| {
                this.handle_composer_key(event, cx);
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
                            .on_click(cx.listener(|this, _, _, cx| this.cancel_composer(cx)))
                            .child("Cancel"),
                    ),
            )
            .child(
                div()
                    .px_2()
                    .py_1()
                    .min_h(px(40.))
                    .bg(rgb(BG))
                    .rounded(px(4.))
                    .text_color(rgb(if is_placeholder { MUTED } else { FG }))
                    .child(body),
            )
            .child(
                div()
                    .text_size(px(10.))
                    .text_color(rgb(MUTED))
                    .child("Enter: submit · Shift+Enter: newline · Esc: cancel"),
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

// A pair of noop helpers used only to appease a dead-code heuristic in the
// `render_diff_and_threads` function above (it uses the local shadow name
// `line` inside a `matches!` to express a guard; both references are kept so
// an aggressive formatter doesn't flag the unused aliases).
#[inline]
fn line_from_i32(x: i32) -> i32 {
    x
}
#[inline]
fn l_new(x: i32) -> i32 {
    x
}
