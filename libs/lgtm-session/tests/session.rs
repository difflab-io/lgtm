//! BDD integration tests for `lgtm-session`.

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
};

use lgtm_session::{
    LgtmSessionError, Session, SessionService, SidebarWidget, TabCandidate, WorkspaceStatePatch,
};
use tempfile::TempDir;
use tracing_subscriber::fmt::MakeWriter;
use uuid::Uuid;

// fixtures --------------------------------------------------------------------

/// Shared in-memory buffer writer passed to `tracing_subscriber` to capture verbose logs during each test.
#[derive(Clone, Default)]
struct BufferWriter {
    buffer: Arc<Mutex<Vec<u8>>>,
}

impl Write for BufferWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.buffer.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for BufferWriter {
    type Writer = Self;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

/// Isolated session test fixture with `.lgtm/sessions`, `.lgtm/worktrees`, and a Git repository:
///
/// ```text
/// <tmp>/
/// ├── .lgtm/
/// │   ├── sessions/
/// │   │   └── default.json         (Persisted Session JSON)
/// │   └── worktrees/
/// │       └── lgtm/
/// │           └── <branch-slug>/   (Managed Worktree)
/// └── lgtm/                        (Project.root_path, branch = "main")
///     ├── .git/
///     └── README.md
/// ```
struct SessionTestFixture {
    tmp: TempDir,
    lgtm_home: PathBuf,
    service: SessionService,
    logs: BufferWriter,
}

impl SessionTestFixture {
    /// Creates an empty isolated session fixture with no persisted `default.json` file yet.
    fn empty() -> Self {
        let tmp = TempDir::new().unwrap();
        let lgtm_home = tmp.path().join(".lgtm");
        let service = SessionService::new(Some(lgtm_home.clone()));
        Self {
            tmp,
            lgtm_home,
            service,
            logs: BufferWriter::default(),
        }
    }

    /// Initializes a primary Git repository (`<tmp>/<name>`) on `main` with an initial commit:
    ///
    /// ```text
    /// <tmp>/<name>/
    /// ├── .git/          (HEAD -> refs/heads/main)
    /// └── README.md
    /// ```
    fn init_git_repo(&self, name: &str) -> PathBuf {
        let repo = self.tmp.path().join(name);
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

    /// Creates a session fixture with an open Git project (`lgtm`) and a managed worktree on `branch`.
    fn with_project_and_worktree(branch: &str) -> (Self, PathBuf, Uuid, PathBuf, Session) {
        let fixture = Self::empty();
        let repo = fixture.init_git_repo("lgtm");
        let opened = fixture.service.open_workspace("default", &repo).unwrap();
        let project_id = opened.projects[0].id;
        let with_wt = fixture
            .service
            .create_project_worktree("default", project_id, branch, None)
            .unwrap();
        let wt_path = PathBuf::from(&with_wt.active_workspace_root);
        (fixture, repo, project_id, wt_path, with_wt)
    }

    /// Runs `f` inside a verbose `TRACE`-level `tracing` subscriber scope.
    fn with_verbose_logger<R>(&self, f: impl FnOnce() -> R) -> R {
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_ansi(false)
            .without_time()
            .with_writer(self.logs.clone())
            .finish();
        tracing::subscriber::with_default(subscriber, f)
    }

    /// Writes initial state, final state, and captured verbose logs/errors to `libs/lgtm-session/test-outputs/<suite>/<case>.log`.
    fn write_artifact(&self, suite: &str, case: &str, initial_state: &str, final_state: &str) {
        let out_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("test-outputs")
            .join(suite);
        fs::create_dir_all(&out_dir).unwrap();
        let captured_logs = String::from_utf8_lossy(&self.logs.buffer.lock().unwrap()).into_owned();
        let artifact = format!(
            "=== INITIAL STATE ===\n{initial_state}\n\n=== FINAL STATE ===\n{final_state}\n\n=== LOGS & ERRORS ===\n{captured_logs}"
        );
        fs::write(out_dir.join(format!("{case}.log")), artifact).unwrap();
    }
}

// tests -----------------------------------------------------------------------

mod given_no_session_exists {
    use super::*;

    mod when_loading_or_mutating_missing_projects {
        use super::*;

        #[test]
        fn should_return_empty_default_session_and_reject_unknown_project_ids() {
            // Arrange
            let fixture = SessionTestFixture::empty();
            let session_file = fixture.lgtm_home.join("sessions").join("default.json");
            let unknown_id = Uuid::new_v4();
            let initial_state = format!("session_file_exists: {}", session_file.exists());

            // Act
            let (empty_session, collapse_err, remove_err) = fixture.with_verbose_logger(|| {
                let empty = fixture.service.get_session("default").unwrap();
                let collapse_err = fixture
                    .service
                    .set_project_collapsed("default", unknown_id, true)
                    .expect_err("unknown project id should fail");
                let remove_err = fixture
                    .service
                    .remove_project("default", unknown_id)
                    .expect_err("unknown project id should fail");
                (empty, collapse_err, remove_err)
            });

            // Assert
            assert_eq!(empty_session.id, "default");
            assert_eq!(empty_session.sidebar_widget, SidebarWidget::Explorer);
            assert!(empty_session.active_workspace_root.is_empty());
            assert!(empty_session.projects.is_empty());
            assert!(empty_session.workspaces.is_empty());
            assert!(matches!(collapse_err, LgtmSessionError::NotFound(_)));
            assert!(matches!(remove_err, LgtmSessionError::NotFound(_)));

            let final_state = serde_json::to_string_pretty(&empty_session).unwrap();
            fixture.write_artifact(
                "given_no_session_exists",
                "should_return_empty_default_session_and_reject_unknown_project_ids",
                &initial_state,
                &final_state,
            );
        }
    }
}

mod given_no_project_exists_in_session {
    use super::*;

    mod when_opening_a_workspace_directory {
        use super::*;

        #[test]
        fn should_register_project_activate_workspace_and_persist_session_file() {
            // Arrange
            let fixture = SessionTestFixture::empty();
            let repo = fixture.init_git_repo("lgtm");
            let initial_session = fixture.service.get_session("default").unwrap();
            let initial_state = serde_json::to_string_pretty(&initial_session).unwrap();

            // Act
            let opened = fixture
                .with_verbose_logger(|| fixture.service.open_workspace("default", &repo).unwrap());

            // Assert
            assert_eq!(opened.projects.len(), 1);
            assert_eq!(opened.projects[0].name, "lgtm");
            assert_eq!(opened.projects[0].branch.as_deref(), Some("main"));
            assert_eq!(opened.active_workspace_root, repo.to_string_lossy());
            assert!(
                opened
                    .workspaces
                    .contains_key(repo.to_string_lossy().as_ref())
            );
            assert!(
                fixture
                    .lgtm_home
                    .join("sessions")
                    .join("default.json")
                    .is_file()
            );

            let final_state = serde_json::to_string_pretty(&opened).unwrap();
            fixture.write_artifact(
                "given_no_project_exists_in_session",
                "should_register_project_activate_workspace_and_persist_session_file",
                &initial_state,
                &final_state,
            );
        }
    }
}

mod given_a_project_exists_in_session {
    use super::*;

    mod when_creating_worktrees_and_saving_workspace_tabs {
        use super::*;

        #[test]
        fn should_retain_only_edited_and_last_opened_tabs_and_persist_sidebar_and_collapsed_state()
        {
            // Arrange
            let (fixture, _, project_id, wt_path, with_wt) =
                SessionTestFixture::with_project_and_worktree("worktree/feature-tabs");
            let wt_path_str = wt_path.to_string_lossy().into_owned();
            let initial_state = serde_json::to_string_pretty(&with_wt).unwrap();

            // Act
            let (collapsed, reloaded) = fixture.with_verbose_logger(|| {
                fixture
                    .service
                    .save_workspace_state(
                        "default",
                        WorkspaceStatePatch {
                            workspace_root: wt_path_str.clone(),
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
                let collapsed = fixture
                    .service
                    .set_project_collapsed("default", project_id, true)
                    .unwrap();
                let reloaded = SessionService::new(Some(fixture.lgtm_home.clone()))
                    .get_session("default")
                    .unwrap();
                (collapsed, reloaded)
            });

            // Assert
            assert!(collapsed.projects[0].collapsed);
            assert_eq!(reloaded.sidebar_widget, SidebarWidget::Projects);
            assert!(reloaded.projects[0].collapsed);
            let ws_state = reloaded.workspaces.get(&wt_path_str).unwrap();
            assert_eq!(ws_state.expanded_dirs, vec!["src".to_owned()]);
            assert_eq!(
                ws_state.last_opened_tab_path.as_deref(),
                Some("src/active.rs")
            );
            assert_eq!(ws_state.tabs.len(), 2);
            assert_eq!(ws_state.tabs[0].path, "src/edited.rs");
            assert!(ws_state.tabs[0].is_edited());
            assert_eq!(ws_state.tabs[1].path, "src/active.rs");
            assert!(ws_state.tabs[1].is_last_opened());

            let final_state = serde_json::to_string_pretty(&reloaded).unwrap();
            fixture.write_artifact(
                "given_a_project_exists_in_session",
                "should_retain_only_edited_and_last_opened_tabs_and_persist_sidebar_and_collapsed_state",
                &initial_state,
                &final_state,
            );
        }
    }

    mod when_updating_project_appearance {
        use super::*;

        #[test]
        fn should_persist_custom_project_name_icon_and_color() {
            // Arrange
            let (fixture, _, project_id, _, initial_session) =
                SessionTestFixture::with_project_and_worktree("worktree/feature-appearance");
            let initial_state = serde_json::to_string_pretty(&initial_session).unwrap();

            // Act
            let reloaded = fixture.with_verbose_logger(|| {
                fixture
                    .service
                    .update_project_appearance(
                        "default",
                        project_id,
                        Some("LGTM Core".to_owned()),
                        Some("rocket".to_owned()),
                        Some("violet".to_owned()),
                    )
                    .unwrap();
                SessionService::new(Some(fixture.lgtm_home.clone()))
                    .get_session("default")
                    .unwrap()
            });

            // Assert
            assert_eq!(reloaded.projects[0].name, "LGTM Core");
            assert_eq!(reloaded.projects[0].icon.as_deref(), Some("rocket"));
            assert_eq!(reloaded.projects[0].color.as_deref(), Some("violet"));

            let final_state = serde_json::to_string_pretty(&reloaded).unwrap();
            fixture.write_artifact(
                "given_a_project_exists_in_session",
                "should_persist_custom_project_name_icon_and_color",
                &initial_state,
                &final_state,
            );
        }
    }

    mod when_renaming_worktrees {
        use super::*;

        #[test]
        fn should_rename_worktree_without_changing_branch_and_reject_missing_worktrees() {
            // Arrange
            let (fixture, _, project_id, wt_path, initial_session) =
                SessionTestFixture::with_project_and_worktree("worktree/feature-rename");
            let initial_state = serde_json::to_string_pretty(&initial_session).unwrap();

            // Act
            let (reloaded, missing_wt_err) = fixture.with_verbose_logger(|| {
                fixture
                    .service
                    .rename_project_worktree("default", project_id, &wt_path, "Auth Refactor")
                    .unwrap();
                let missing_wt_err = fixture
                    .service
                    .rename_project_worktree(
                        "default",
                        project_id,
                        Path::new("/nonexistent/worktree"),
                        "Missing",
                    )
                    .expect_err("renaming nonexistent worktree should fail");
                let reloaded = SessionService::new(Some(fixture.lgtm_home.clone()))
                    .get_session("default")
                    .unwrap();
                (reloaded, missing_wt_err)
            });

            // Assert
            assert!(matches!(missing_wt_err, LgtmSessionError::NotFound(_)));
            assert_eq!(reloaded.projects[0].worktrees[0].name, "Auth Refactor");
            assert_eq!(
                reloaded.projects[0].worktrees[0].branch.as_deref(),
                Some("worktree/feature-rename")
            );

            let final_state = serde_json::to_string_pretty(&reloaded).unwrap();
            fixture.write_artifact(
                "given_a_project_exists_in_session",
                "should_rename_worktree_without_changing_branch_and_reject_missing_worktrees",
                &initial_state,
                &final_state,
            );
        }
    }

    mod when_deleting_worktrees_and_removing_projects {
        use super::*;

        #[test]
        fn should_delete_managed_worktree_storage_and_remove_project_state() {
            // Arrange
            let (fixture, repo, project_id, mgd_path, initial_session) =
                SessionTestFixture::with_project_and_worktree("worktree/feature-mgd");
            fs::write(mgd_path.join("untracked.txt"), "dirty").unwrap();
            let initial_state = serde_json::to_string_pretty(&initial_session).unwrap();

            // Act
            let (dirty_err, after_wt_removal, after_project_removal) =
                fixture.with_verbose_logger(|| {
                    let dirty_err = fixture
                        .service
                        .remove_project_worktree("default", project_id, &mgd_path, false)
                        .expect_err("dirty managed worktree should require force");
                    let after_wt_removal = fixture
                        .service
                        .remove_project_worktree("default", project_id, &mgd_path, true)
                        .unwrap();
                    let after_project_removal = fixture
                        .service
                        .remove_project("default", project_id)
                        .unwrap();
                    (dirty_err, after_wt_removal, after_project_removal)
                });

            // Assert
            assert!(matches!(
                dirty_err,
                LgtmSessionError::Project(lgtm_project::LgtmProjectError::DirtyWorktree(_))
            ));
            assert!(!mgd_path.exists());
            assert_eq!(
                after_wt_removal.active_workspace_root,
                repo.to_string_lossy().as_ref()
            );
            assert!(after_wt_removal.projects[0].worktrees.is_empty());
            assert!(after_project_removal.projects.is_empty());
            assert!(after_project_removal.workspaces.is_empty());
            assert!(after_project_removal.active_workspace_root.is_empty());

            let final_state = serde_json::to_string_pretty(&after_project_removal).unwrap();
            fixture.write_artifact(
                "given_a_project_exists_in_session",
                "should_delete_managed_worktree_storage_and_remove_project_state",
                &initial_state,
                &final_state,
            );
        }
    }
}
