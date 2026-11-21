//! BDD integration tests for `lgtm-session`.

use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::Command,
    sync::{Arc, Mutex},
};

use lgtm_project::ProjectService;
use lgtm_session::{Session, SessionService};
use lgtm_workspace::{SidebarWidget, TabCandidate, WorkspaceStatePatch};
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
    projects: ProjectService,
    logs: BufferWriter,
}

impl SessionTestFixture {
    /// Creates an empty isolated session fixture with no persisted `default.json` file yet.
    fn empty() -> Self {
        let tmp = TempDir::new().unwrap();
        let lgtm_home = tmp.path().join(".lgtm");
        let service = SessionService::new(Some(lgtm_home.clone()));
        let projects = ProjectService::new(Some(lgtm_home.clone()));
        Self {
            tmp,
            lgtm_home,
            service,
            projects,
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

    /// Creates a session fixture with a persisted Git project (`lgtm`) and managed worktree on `branch`.
    fn with_project_and_worktree(branch: &str) -> (Self, PathBuf, Uuid, PathBuf, Session) {
        let fixture = Self::empty();
        let repo = fixture.init_git_repo("lgtm");
        let mut session = fixture.service.get_session("default").unwrap();
        let root = fixture
            .projects
            .open_project(&mut session.projects, &repo)
            .unwrap();
        session.set_active_workspace(root);
        let project_id = session.projects[0].id;
        let wt = fixture
            .projects
            .create_project_worktree(&mut session.projects, project_id, branch, None)
            .unwrap();
        let wt_path = PathBuf::from(&wt.path);
        session.set_active_workspace(wt.path);
        fixture.service.save_session(&session).unwrap();
        (fixture, repo, project_id, wt_path, session)
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

    mod when_loading_default_session {
        use super::*;

        #[test]
        fn should_return_empty_default_session_without_writing_disk_file() {
            // Arrange
            let fixture = SessionTestFixture::empty();
            let session_file = fixture.lgtm_home.join("sessions").join("default.json");
            let initial_state = format!("session_file_exists: {}", session_file.exists());

            // Act
            let empty_session =
                fixture.with_verbose_logger(|| fixture.service.get_session("default").unwrap());

            // Assert
            assert_eq!(empty_session.id, "default");
            assert_eq!(empty_session.sidebar_widget, SidebarWidget::Explorer);
            assert!(empty_session.active_workspace_root.is_empty());
            assert!(empty_session.projects.is_empty());
            assert!(empty_session.workspaces.is_empty());
            assert!(!session_file.exists());

            let final_state = serde_json::to_string_pretty(&empty_session).unwrap();
            fixture.write_artifact(
                "given_no_session_exists",
                "should_return_empty_default_session_without_writing_disk_file",
                &initial_state,
                &final_state,
            );
        }
    }
}

mod given_no_project_exists_in_session {
    use super::*;

    mod when_saving_a_session_with_an_opened_workspace {
        use super::*;

        #[test]
        fn should_persist_and_reload_project_and_active_workspace() {
            // Arrange
            let fixture = SessionTestFixture::empty();
            let repo = fixture.init_git_repo("lgtm");
            let mut session = fixture.service.get_session("default").unwrap();
            let initial_state = serde_json::to_string_pretty(&session).unwrap();

            // Act
            let reloaded = fixture.with_verbose_logger(|| {
                let target = fixture
                    .projects
                    .open_project(&mut session.projects, &repo)
                    .unwrap();
                session.set_active_workspace(target);
                fixture.service.save_session(&session).unwrap();
                SessionService::new(Some(fixture.lgtm_home.clone()))
                    .get_session("default")
                    .unwrap()
            });

            // Assert
            assert_eq!(reloaded.projects.len(), 1);
            assert_eq!(reloaded.projects[0].name, "lgtm");
            assert_eq!(reloaded.projects[0].branch.as_deref(), Some("main"));
            assert_eq!(reloaded.active_workspace_root, repo.to_string_lossy());
            assert!(
                reloaded
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

            let final_state = serde_json::to_string_pretty(&reloaded).unwrap();
            fixture.write_artifact(
                "given_no_project_exists_in_session",
                "should_persist_and_reload_project_and_active_workspace",
                &initial_state,
                &final_state,
            );
        }
    }
}

mod given_a_project_exists_in_session {
    use super::*;

    mod when_saving_workspace_tabs_and_sidebar_widget {
        use super::*;

        #[test]
        fn should_retain_only_edited_and_last_opened_tabs_and_persist_sidebar_state() {
            // Arrange
            let (fixture, _, _, wt_path, with_wt) =
                SessionTestFixture::with_project_and_worktree("worktree/feature-tabs");
            let wt_path_str = wt_path.to_string_lossy().into_owned();
            let initial_state = serde_json::to_string_pretty(&with_wt).unwrap();

            // Act
            let reloaded = fixture.with_verbose_logger(|| {
                fixture
                    .service
                    .save_workspace_state(
                        "default",
                        WorkspaceStatePatch {
                            workspace_root: wt_path_str.clone(),
                            tabs: vec![
                                TabCandidate {
                                    path: "   ".to_owned(),
                                    title: "blank".to_owned(),
                                    edited: true,
                                    last_opened: true,
                                },
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
                SessionService::new(Some(fixture.lgtm_home.clone()))
                    .get_session("default")
                    .unwrap()
            });

            // Assert
            assert_eq!(reloaded.sidebar_widget, SidebarWidget::Projects);
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
                "should_retain_only_edited_and_last_opened_tabs_and_persist_sidebar_state",
                &initial_state,
                &final_state,
            );
        }
    }

    mod when_applying_worktree_and_project_removals {
        use super::*;

        #[test]
        fn should_prune_workspace_entries_and_fall_back_active_workspace_root() {
            // Arrange
            let (fixture, repo, project_id, mgd_path, mut session) =
                SessionTestFixture::with_project_and_worktree("worktree/feature-mgd");
            let initial_state = serde_json::to_string_pretty(&session).unwrap();

            // Act
            let (after_wt_removal, after_project_removal) = fixture.with_verbose_logger(|| {
                let (canonical_str, raw_str, parent_root) = fixture
                    .projects
                    .remove_project_worktree(&mut session.projects, project_id, &mgd_path, true)
                    .unwrap();
                session.apply_worktree_removal(&canonical_str, &raw_str, parent_root);
                fixture.service.save_session(&session).unwrap();
                let after_wt_removal = fixture.service.get_session("default").unwrap();

                let removed = fixture
                    .projects
                    .remove_project(&mut session.projects, project_id)
                    .unwrap();
                session.apply_project_removal(&removed);
                fixture.service.save_session(&session).unwrap();
                let after_project_removal = fixture.service.get_session("default").unwrap();
                (after_wt_removal, after_project_removal)
            });

            // Assert
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
                "should_prune_workspace_entries_and_fall_back_active_workspace_root",
                &initial_state,
                &final_state,
            );
        }
    }
}
