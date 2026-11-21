//! Diff and review mode (`DiffService` and `ReviewService`) IPC command handlers.

use lgtm_desktop::{DiffFileEntry, DiffWorkspaceSnapshot, FileDiffContent, ReviewMode};
use lgtm_workspace::Result;
use tauri::State;

use crate::app::AppState;

/// Switches the active [`ReviewMode`] (`Local` vs. `ForgeBacked`) and returns the refreshed workspace diff snapshot.
#[tauri::command]
#[specta::specta]
pub async fn review_set_mode(
    state: State<'_, AppState>,
    mode: ReviewMode,
) -> Result<DiffWorkspaceSnapshot> {
    state
        .run_blocking(move |app| {
            let updated_mode = app.review.set_mode(mode);
            let w = app.lock_workspace()?;
            let root = w.get_root()?;
            app.diff.get_state(&root, w.get_lgtm_home(), updated_mode)
        })
        .await
}

/// Returns the active [`ReviewMode`], branch name, commit checklist, default selection, and changed file list.
#[tauri::command]
#[specta::specta]
pub async fn diff_get_state(state: State<'_, AppState>) -> Result<DiffWorkspaceSnapshot> {
    state
        .run_blocking(move |app| {
            let w = app.lock_workspace()?;
            let root = w.get_root()?;
            app.diff
                .get_state(&root, w.get_lgtm_home(), app.review.get_mode())
        })
        .await
}

/// Computes changed files and line deltas across `commit_ids` (`oldest.base_sha .. newest.head_sha`).
#[tauri::command]
#[specta::specta]
pub async fn diff_list_files(
    state: State<'_, AppState>,
    commit_ids: Vec<String>,
) -> Result<Vec<DiffFileEntry>> {
    state
        .run_blocking(move |app| {
            let w = app.lock_workspace()?;
            let root = w.get_root()?;
            app.diff.get_files(&root, w.get_lgtm_home(), &commit_ids)
        })
        .await
}

/// Reads `base_content` and `head_content` for `path` across `commit_ids`.
#[tauri::command]
#[specta::specta]
pub async fn diff_read_file(
    state: State<'_, AppState>,
    path: String,
    commit_ids: Vec<String>,
) -> Result<FileDiffContent> {
    state
        .run_blocking(move |app| {
            let w = app.lock_workspace()?;
            let root = w.get_root()?;
            app.diff
                .get_file_diff(&root, w.get_lgtm_home(), &path, &commit_ids)
        })
        .await
}
