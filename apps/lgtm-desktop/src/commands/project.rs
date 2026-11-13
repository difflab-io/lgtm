//! Project and managed Git worktree IPC command handlers delegating to `SessionService`.

use std::path::Path;

use lgtm_session::Session;
use lgtm_workspace::{Result, error};
use tauri::State;
use uuid::Uuid;

use crate::app::AppState;

/// Collapses or expands child worktrees for `project_id` in the `default` session.
#[tauri::command]
#[specta::specta]
pub async fn project_set_collapsed(
    state: State<'_, AppState>,
    project_id: String,
    collapsed: bool,
) -> Result<Session> {
    state
        .run_blocking(move |app| {
            app.mutate_session(move |s| {
                Ok(s.set_project_collapsed("default", parse_project_id(&project_id)?, collapsed)?)
            })
        })
        .await
}

/// Updates custom `name`, `icon`, and `color` metadata for `project_id` in the `default` session.
#[tauri::command]
#[specta::specta]
pub async fn project_update_appearance(
    state: State<'_, AppState>,
    project_id: String,
    name: Option<String>,
    icon: Option<String>,
    color: Option<String>,
) -> Result<Session> {
    state
        .run_blocking(move |app| {
            app.mutate_session(move |s| {
                Ok(s.update_project_appearance(
                    "default",
                    parse_project_id(&project_id)?,
                    name,
                    icon,
                    color,
                )?)
            })
        })
        .await
}

/// Renames a worktree display label under `project_id` without changing its Git branch name.
#[tauri::command]
#[specta::specta]
pub async fn project_rename_worktree(
    state: State<'_, AppState>,
    project_id: String,
    worktree_path: String,
    name: String,
) -> Result<Session> {
    state
        .run_blocking(move |app| {
            app.mutate_session(move |s| {
                Ok(s.rename_project_worktree(
                    "default",
                    parse_project_id(&project_id)?,
                    Path::new(worktree_path.trim()),
                    &name,
                )?)
            })
        })
        .await
}

/// Creates a `Managed` Git worktree under `~/.lgtm/worktrees/<project>/<branch-slug>` and attaches it to the `default` session.
#[tauri::command]
#[specta::specta]
pub async fn project_create_worktree(
    state: State<'_, AppState>,
    project_id: String,
    branch: String,
    base_ref: Option<String>,
) -> Result<Session> {
    state
        .run_blocking(move |app| {
            app.mutate_session(move |s| {
                Ok(s.create_project_worktree(
                    "default",
                    parse_project_id(&project_id)?,
                    &branch,
                    base_ref.as_deref(),
                )?)
            })
        })
        .await
}

/// Removes a managed worktree from `project_id` and deletes its checkout directory on disk.
#[tauri::command]
#[specta::specta]
pub async fn project_remove_worktree(
    state: State<'_, AppState>,
    project_id: String,
    worktree_path: String,
    force: Option<bool>,
) -> Result<Session> {
    state
        .run_blocking(move |app| {
            app.mutate_session(move |s| {
                Ok(s.remove_project_worktree(
                    "default",
                    parse_project_id(&project_id)?,
                    Path::new(worktree_path.trim()),
                    force.unwrap_or(false),
                )?)
            })
        })
        .await
}

/// Removes `project_id` and its remembered workspace states from the `default` session.
#[tauri::command]
#[specta::specta]
pub async fn project_remove(state: State<'_, AppState>, project_id: String) -> Result<Session> {
    state
        .run_blocking(move |app| {
            app.mutate_session(move |s| {
                Ok(s.remove_project("default", parse_project_id(&project_id)?)?)
            })
        })
        .await
}

/// Switches `workspace_root` to `branch` (or creates `-b <branch>` when `create` is `true`) and returns the refreshed session.
#[tauri::command]
#[specta::specta]
pub async fn project_checkout_branch(
    state: State<'_, AppState>,
    workspace_root: String,
    branch: String,
    create: Option<bool>,
) -> Result<Session> {
    state
        .run_blocking(move |app| {
            app.mutate_session(move |s| {
                Ok(s.checkout_workspace_branch(
                    "default",
                    Path::new(workspace_root.trim()),
                    &branch,
                    create.unwrap_or(false),
                )?)
            })
        })
        .await
}

fn parse_project_id(project_id: &str) -> Result<Uuid> {
    Uuid::parse_str(project_id.trim()).map_err(|err| error("invalidProject", err.to_string()))
}
