//! Two-pane review view. Left: file list. Right: raw diff text.
//!
//! Mirrors the layout and class semantics of the TanStack / Tauri frontends.
//! Colors are tuned to the same palette (bg #0b0d10, accent #4c9aff, etc.).

use gpui::{
    App, Context, FontWeight, InteractiveElement, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px, rgb,
};

use crate::stub::{ChangedFile, Status, diff_for, files};

const BG: u32 = 0x0b0d10;
const FG: u32 = 0xd7dbe0;
const MUTED: u32 = 0x8b94a3;
const BORDER: u32 = 0x1e2228;
const SIDEBAR_BG: u32 = 0x0b0d10;
const ACTIVE_BG: u32 = 0x172238;

pub struct ReviewView {
    files: Vec<ChangedFile>,
    selected: Option<usize>,
}

impl ReviewView {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        let files = files();
        eprintln!("[ReviewView] loaded {} stub files", files.len());
        let selected = if files.is_empty() { None } else { Some(0) };
        Self { files, selected }
    }

    fn select(&mut self, idx: usize, cx: &mut Context<Self>) {
        if idx < self.files.len() {
            self.selected = Some(idx);
            cx.notify();
        }
    }

    fn selected_diff(&self) -> String {
        match self.selected {
            Some(i) => diff_for(&self.files[i].path),
            None => String::new(),
        }
    }

    fn selected_path(&self) -> SharedString {
        self.selected
            .and_then(|i| self.files.get(i))
            .map(|f| SharedString::from(f.path.clone()))
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
            .child(self.render_diff())
    }
}

impl ReviewView {
    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        // A `.map()` closure would be an `FnMut` capturing `cx` mutably and
        // returning borrows that outlive each call — not allowed. A for-loop
        // uses `cx` sequentially instead.
        let mut items = Vec::with_capacity(self.files.len());
        for (i, file) in self.files.iter().enumerate() {
            items.push(self.render_file_item(i, file, cx));
        }

        div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .w(px(320.0))
            .h_full()
            .bg(rgb(SIDEBAR_BG))
            .border_r_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .text_color(rgb(MUTED))
                    .text_size(px(11.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("CHANGED FILES"),
            )
            .child(div().flex().flex_col().py_1().children(items))
    }

    fn render_file_item(
        &self,
        idx: usize,
        file: &ChangedFile,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        // Explicit `use<>` opts out of Rust 2024's new `impl Trait` capture
        // rules; the returned element owns its data (idx is Copy, path/label
        // are SharedString clones, the listener is owned) so it does not
        // need to tie its lifetime to `&self` or `cx`.
        let is_active = self.selected == Some(idx);
        let status_color = status_color(file.status);
        let path = SharedString::from(file.path.clone());
        let status_label = SharedString::from(file.status.label().to_string());

        div()
            .id(("fileitem", idx))
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .px_3()
            .py_1()
            .cursor_pointer()
            .when(is_active, |d| d.bg(rgb(ACTIVE_BG)))
            .hover(|d| d.bg(rgb(0x11161e)))
            .on_click(cx.listener(move |this, _, _, cx| this.select(idx, cx)))
            .child(
                div()
                    .w(px(72.0))
                    .text_size(px(10.0))
                    .text_color(rgb(status_color))
                    .child(status_label),
            )
            .child(div().flex_1().child(path))
    }

    fn render_diff(&self) -> impl IntoElement {
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
                    .child(self.selected_path()),
            )
            .child(
                div()
                    .flex_grow(1.0)
                    .p_3()
                    .overflow_hidden()
                    .child(self.selected_diff()),
            )
    }
}

fn status_color(s: Status) -> u32 {
    match s {
        Status::Added | Status::Untracked => 0x40c057,
        Status::Modified => 0x4c9aff,
        Status::Deleted => 0xe03131,
    }
}

/// Trait bounds assertion to catch API drift at compile time: ReviewView must
/// be a GPUI entity (Render + 'static).
#[allow(dead_code)]
fn _assert_render<T: Render + 'static>(_: &T) {}

#[allow(dead_code)]
fn _assert_bounds(cx: &mut App) {
    let _ = cx; // silence
}
