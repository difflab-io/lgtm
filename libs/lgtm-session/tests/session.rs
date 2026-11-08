//! BDD integration tests for `lgtm-session`.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use lgtm_rsutils::add_worktree;
use lgtm_session::{
    LgtmSessionError, SessionService, SidebarWidget, TabCandidate, WorkspaceStatePatch,
};
use tempfile::TempDir;

// fixtures --------------------------------------------------------------------

fn init_git_repo(dir: &Path, name: &str) -> PathBuf {
    let repo = dir.join(name);
    fs::create_dir_all(&repo).unwrap();
    Command::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["init", "-b", "main"])
        .output()
        .unwrap();
    Command::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["config", "user.name", "Test User"])
        .output()
        .unwrap();
    Command::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["config", "user.email", "test@example.com"])
        .output()
        .unwrap();
    fs::write(repo.join("README.md"), "# Repo\n").unwrap();
    Command::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["add", "README.md"])
        .output()
        .unwrap();
    Command::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["commit", "-m", "Initial commit"])
        .output()
        .unwrap();
    fs::canonicalize(repo).unwrap()
}

// tests -----------------------------------------------------------------------

mod given_a_default_session {
    use super::*;

    mod when_opening_projects_and_saving_workspace_tabs {
        use super::*;

        #[test]
        fn should_persist_projects_collapsed_state_and_only_edited_or_last_opened_tabs() {
            // Arrange
            let tmp = TempDir::new().unwrap();
            let lgtm_home = tmp.path().join(".lgtm");
            let repo = init_git_repo(tmp.path(), "lgtm");
            let service = SessionService::new(Some(lgtm_home.clone()));

            // Act
            let opened = service.open_workspace("default", &repo).unwrap();
            let project_id = opened.projects[0].id;
            let with_wt = service
                .create_project_worktree("default", project_id, "worktree/feature-tabs", None)
                .unwrap();
            let wt_path = with_wt.active_workspace_root.clone();
            service
                .save_workspace_state(
                    "default",
                    WorkspaceStatePatch {
                        workspace_root: wt_path.clone(),
                        tabs: vec![
                            TabCandidate {
                                path: "README.md".to_owned(),
                                title: "README.md".to_owned(),
                                edited: false,
                                last_opened: false,
                            },
                            TabCandidate {
                                path: "src/edited.rs".to_owned(),
                                title: "edited.rs".to_owned(),
                                edited: true,
                                last_opened: false,
                            },
                            TabCandidate {
                                path: "src/active.rs".to_owned(),
                                title: "active.rs".to_owned(),
                                edited: false,
                                last_opened: true,
                            },
                        ],
                        expanded_dirs: vec!["src".to_owned()],
                        sidebar_widget: Some(SidebarWidget::Projects),
                    },
                )
                .unwrap();
            let collapsed = service
                .set_project_collapsed("default", project_id, true)
                .unwrap();
            let reloaded = SessionService::new(Some(lgtm_home))
                .get_session("default")
                .unwrap();

            // Assert
            assert!(collapsed.projects[0].collapsed);
            assert_eq!(reloaded.sidebar_widget, SidebarWidget::Projects);
            assert!(reloaded.projects[0].collapsed);
            let ws_state = reloaded.workspaces.get(&wt_path).unwrap();
            assert_eq!(ws_state.expanded_dirs, vec!["src".to_owned()]);
            assert_eq!(
                ws_state.last_opened_tab_path.as_deref(),
                Some("src/active.rs")
            );
            assert_eq!(ws_state.tabs.len(), 2);
            assert_eq!(ws_state.tabs[0].path, "src/edited.rs");
            assert_eq!(ws_state.tabs[1].path, "src/active.rs");
        }
    }

    mod when_removing_managed_and_external_worktrees {
        use super::*;

        #[test]
        fn should_delete_managed_worktree_storage_and_only_detach_external_worktree() {
            // Arrange
            let tmp = TempDir::new().unwrap();
            let lgtm_home = tmp.path().join(".lgtm");
            let repo = init_git_repo(tmp.path(), "lgtm");
            let ext_dir = tmp
                .path()
                .join(".herdr")
                .join("worktrees")
                .join("lgtm")
                .join("feature-ext");
            let ext_entry = add_worktree(&repo, &ext_dir, "worktree/feature-ext", None).unwrap();
            let service = SessionService::new(Some(lgtm_home));

            // Act
            let s1 = service.open_workspace("default", &ext_entry.path).unwrap();
            let project_id = s1.projects[0].id;
            let s2 = service
                .create_project_worktree("default", project_id, "worktree/feature-mgd", None)
                .unwrap();
            let mgd_path = PathBuf::from(&s2.active_workspace_root);
            fs::write(mgd_path.join("untracked.txt"), "dirty").unwrap();
            let dirty_err = service
                .remove_project_worktree("default", project_id, &mgd_path, false)
                .expect_err("dirty managed worktree should require force");
            let s3 = service
                .remove_project_worktree("default", project_id, &mgd_path, true)
                .unwrap();
            let s4 = service
                .remove_project_worktree("default", project_id, &ext_entry.path, false)
                .unwrap();

            // Assert
            assert!(matches!(
                dirty_err,
                LgtmSessionError::Project(lgtm_project::LgtmProjectError::DirtyWorktree(_))
            ));
            assert!(!mgd_path.exists());
            assert_eq!(s3.active_workspace_root, repo.to_string_lossy().as_ref());
            assert!(s4.projects[0].worktrees.is_empty());
            assert!(ext_entry.path.exists());
        }
    }
}
