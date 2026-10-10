//! `FileTree` — the left pane as its own `Entity`.
//!
//! Isolating the file list means hover re-paints on its items don't
//! invalidate `DiffView` or the threads sidebar. The tree keeps its own
//! `selected` and reaches into the `DiffView` entity to switch files.

use anyhow::Result;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::diff_view::DiffView;
use crate::proto::ChangedFile;

const FG: u32 = 0xd7dbe0;
const MUTED: u32 = 0x8b94a3;
const BORDER: u32 = 0x1e2228;
const ACCENT: u32 = 0x4c9aff;

pub struct FileTree {
    diff_view: Entity<DiffView>,
    files: Vec<ChangedFile>,
    selected: Option<String>,
}

impl FileTree {
    pub fn new(diff_view: Entity<DiffView>) -> Self {
        Self { diff_view, files: Vec::new(), selected: None }
    }

    pub fn set_files(&mut self, files: Vec<ChangedFile>, cx: &mut Context<Self>) {
        self.files = files;
        cx.notify();
    }

    pub fn select_initial(&mut self, cx: &mut Context<Self>) {
        if self.selected.is_some() {
            return;
        }
        if let Some(first) = self.files.first().cloned() {
            self.select(first.path, cx);
        }
    }

    fn select(&mut self, path: String, cx: &mut Context<Self>) {
        self.selected = Some(path.clone());
        self.diff_view
            .update(cx, |d, cx| d.set_selected_file(path, cx));
        cx.notify();
    }
}

impl Render for FileTree {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if std::env::var("LGTM_TRACE_RENDER").is_ok() {
            use std::sync::atomic::{AtomicU64, Ordering};
            static N: AtomicU64 = AtomicU64::new(0);
            let n = N.fetch_add(1, Ordering::Relaxed) + 1;
            eprintln!("[lgtm-gpui] FileTree render #{n} files={}", self.files.len());
        }

        let mut items = Vec::with_capacity(self.files.len());
        for (i, file) in self.files.iter().enumerate() {
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
}

impl FileTree {
    fn render_file_item(
        &self,
        idx: usize,
        file: &ChangedFile,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let is_active = self.selected.as_deref() == Some(&file.path);
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
                this.select(path_for_click.clone(), cx);
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

fn status_color(status: &str) -> u32 {
    match status {
        "added" | "untracked" => 0x40c057,
        "modified" => ACCENT,
        "deleted" => 0xe03131,
        _ => MUTED,
    }
}

// Silence unused-import warnings when the file is included.
#[allow(dead_code)]
fn _keep_fg_usage() -> u32 {
    FG
}

#[allow(dead_code)]
fn _keep_result() -> Result<()> {
    Ok(())
}
