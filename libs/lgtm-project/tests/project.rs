//! BDD integration tests for `lgtm-project`.

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
};

use lgtm_project::{LgtmProjectError, Project, ProjectService};
use tempfile::TempDir;
use tracing_subscriber::fmt::MakeWriter;

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

/// Isolated project test fixture with temporary `.lgtm/worktrees` and an optional Git repository:
///
/// ```text
/// <tmp>/
/// ├── .lgtm/
/// │   └── worktrees/
/// │       └── lgtm/
/// │           └── <branch-slug>/   (Managed Worktree)
/// └── lgtm/                        (Project.root_path, branch = "main")
///     ├── .git/
///     └── README.md
/// ```
struct ProjectTestFixture {
    tmp: TempDir,
    service: ProjectService,
    logs: BufferWriter,
}

impl ProjectTestFixture {
    /// Creates an empty isolated fixture with `.lgtm/worktrees` and no initialized repository on disk.
    fn empty() -> Self {
        let tmp = TempDir::new().unwrap();
        let lgtm_home = tmp.path().join(".lgtm");
        let service = ProjectService::new(Some(lgtm_home));
        Self {
            tmp,
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

    /// Writes initial state, final state, and captured verbose logs/errors to `libs/lgtm-project/test-outputs/<suite>/<case>.log`.
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

mod given_no_project_exists {
    use super::*;

    mod when_resolving_missing_or_plain_directories {
        use super::*;

        #[test]
        fn should_reject_missing_paths_and_resolve_non_git_directory_without_branch() {
            // Arrange
            let fixture = ProjectTestFixture::empty();
            let missing_path = fixture.tmp.path().join("does-not-exist");
            let plain_dir = fixture.tmp.path().join("notes-folder");
            fs::create_dir_all(&plain_dir).unwrap();
            let initial_state = format!(
                "missing_path_exists: {}\nplain_dir_has_git: {}",
                missing_path.exists(),
                plain_dir.join(".git").exists()
            );

            // Act
            let (missing_err, resolved_plain, create_wt_err) = fixture.with_verbose_logger(|| {
                let missing_err = fixture
                    .service
                    .resolve_project(&missing_path)
                    .expect_err("missing path should fail");
                let (plain_project, linked_wt) =
                    fixture.service.resolve_project(&plain_dir).unwrap();
                let create_wt_err = fixture
                    .service
                    .create_worktree(&plain_project, "worktree/feature-x", None)
                    .expect_err("non-git project cannot create worktrees");
                (missing_err, (plain_project, linked_wt), create_wt_err)
            });

            // Assert
            let (plain_project, linked_wt) = resolved_plain;
            assert!(matches!(missing_err, LgtmProjectError::InvalidPath(_)));
            assert!(linked_wt.is_none());
            assert_eq!(plain_project.name, "notes-folder");
            assert!(plain_project.branch.is_none());
            assert!(plain_project.git_common_dir.is_none());
            assert!(matches!(
                create_wt_err,
                LgtmProjectError::NotGitRepository(_)
            ));

            let final_state = serde_json::to_string_pretty(&plain_project).unwrap();
            fixture.write_artifact(
                "given_no_project_exists",
                "should_reject_missing_paths_and_resolve_non_git_directory_without_branch",
                &initial_state,
                &final_state,
            );
        }
    }
}

mod given_a_git_project_exists {
    use super::*;

    mod when_resolving_primary_and_managed_worktrees {
        use super::*;

        #[test]
        fn should_group_managed_worktrees_under_parent_project() {
            // Arrange
            let fixture = ProjectTestFixture::empty();
            let main_repo = fixture.init_git_repo("lgtm");
            let (primary_project, _) = fixture.service.resolve_project(&main_repo).unwrap();
            let created_wt = fixture
                .service
                .create_worktree(&primary_project, "worktree/feature-managed", None)
                .unwrap();
            let initial_state = format!(
                "main_repo: {}\nmanaged_worktree: {}",
                main_repo.display(),
                created_wt.path
            );

            // Act
            let (project_from_wt, resolved_wt) = fixture.with_verbose_logger(|| {
                fixture
                    .service
                    .resolve_project(Path::new(&created_wt.path))
                    .unwrap()
            });

            // Assert
            assert_eq!(
                project_from_wt.root_path,
                main_repo.to_string_lossy().as_ref()
            );
            assert_eq!(project_from_wt.branch.as_deref(), Some("main"));
            assert!(project_from_wt.contains_workspace(&created_wt.path));
            let wt = resolved_wt.expect("should return managed worktree");
            assert_eq!(wt.name, "feature-managed");
            assert_eq!(wt.branch.as_deref(), Some("worktree/feature-managed"));

            let final_state = serde_json::to_string_pretty(&project_from_wt).unwrap();
            fixture.write_artifact(
                "given_a_git_project_exists",
                "should_group_managed_worktrees_under_parent_project",
                &initial_state,
                &final_state,
            );
        }
    }

    mod when_creating_worktrees {
        use super::*;

        #[test]
        fn should_create_managed_worktree_under_lgtm_home_and_reject_invalid_or_duplicate_branches()
        {
            // Arrange
            let fixture = ProjectTestFixture::empty();
            let main_repo = fixture.init_git_repo("lgtm");
            let (mut project, _) = fixture.service.resolve_project(&main_repo).unwrap();
            let initial_state = serde_json::to_string_pretty(&project).unwrap();

            // Act
            let (managed_wt, invalid_err, dup_err) = fixture.with_verbose_logger(|| {
                let managed_wt = fixture
                    .service
                    .create_worktree(&project, "worktree/feature-managed", None)
                    .unwrap();
                let invalid_err = fixture
                    .service
                    .create_worktree(&project, "bad..branch", None)
                    .expect_err("invalid branch should fail");
                let dup_err = fixture
                    .service
                    .create_worktree(&project, "main", None)
                    .expect_err("checked-out branch should fail");
                (managed_wt, invalid_err, dup_err)
            });
            project.upsert_worktree(managed_wt.clone());

            // Assert
            assert_eq!(managed_wt.name, "feature-managed");
            assert_eq!(
                managed_wt.branch.as_deref(),
                Some("worktree/feature-managed")
            );
            assert!(Path::new(&managed_wt.path).is_dir());
            assert!(matches!(invalid_err, LgtmProjectError::InvalidBranch(_)));
            assert!(matches!(
                dup_err,
                LgtmProjectError::BranchAlreadyCheckedOut(_)
            ));

            let final_state = serde_json::to_string_pretty(&project).unwrap();
            fixture.write_artifact(
                "given_a_git_project_exists",
                "should_create_managed_worktree_under_lgtm_home_and_reject_invalid_or_duplicate_branches",
                &initial_state,
                &final_state,
            );
        }
    }

    mod when_updating_and_renaming_projects_and_worktrees {
        use super::*;

        #[test]
        fn should_update_project_appearance_and_preserve_custom_worktree_names_across_refresh() {
            // Arrange
            let fixture = ProjectTestFixture::empty();
            let main_repo = fixture.init_git_repo("lgtm");
            let (mut project, _) = fixture.service.resolve_project(&main_repo).unwrap();
            let wt = fixture
                .service
                .create_worktree(&project, "worktree/feature-ui", None)
                .unwrap();
            project.upsert_worktree(wt.clone());
            let initial_state = serde_json::to_string_pretty(&project).unwrap();

            // Act
            fixture.with_verbose_logger(|| {
                project.update_appearance(
                    Some("LGTM Core".to_owned()),
                    Some("rocket".to_owned()),
                    Some("violet".to_owned()),
                );
                assert!(project.rename_worktree(&wt.path, "UI Polish"));
                fixture.service.refresh_project(&mut project).unwrap();
            });

            // Assert
            assert_eq!(project.name, "LGTM Core");
            assert_eq!(project.icon.as_deref(), Some("rocket"));
            assert_eq!(project.color.as_deref(), Some("violet"));
            assert_eq!(project.worktrees[0].name, "UI Polish");
            assert!(project.worktrees[0].has_custom_name());
            assert_eq!(
                project.worktrees[0].branch.as_deref(),
                Some("worktree/feature-ui")
            );
            assert_eq!(Project::default_name(&project.root_path), "lgtm");

            let final_state = serde_json::to_string_pretty(&project).unwrap();
            fixture.write_artifact(
                "given_a_git_project_exists",
                "should_update_project_appearance_and_preserve_custom_worktree_names_across_refresh",
                &initial_state,
                &final_state,
            );
        }
    }

    mod when_deleting_worktrees {
        use super::*;

        #[test]
        fn should_require_force_for_dirty_managed_worktree_and_remove_when_forced() {
            // Arrange
            let fixture = ProjectTestFixture::empty();
            let main_repo = fixture.init_git_repo("lgtm");
            let (mut project, _) = fixture.service.resolve_project(&main_repo).unwrap();
            let managed = fixture
                .service
                .create_worktree(&project, "worktree/feature-clean", None)
                .unwrap();
            project.upsert_worktree(managed.clone());
            fs::write(Path::new(&managed.path).join("dirty.txt"), "uncommitted").unwrap();
            let initial_state = serde_json::to_string_pretty(&project).unwrap();

            // Act
            let dirty_err = fixture.with_verbose_logger(|| {
                let dirty_err = fixture
                    .service
                    .remove_worktree(&project, Path::new(&managed.path), false)
                    .expect_err("dirty managed worktree without force should fail");
                fixture
                    .service
                    .remove_worktree(&project, Path::new(&managed.path), true)
                    .unwrap();
                fixture.service.refresh_project(&mut project).unwrap();
                dirty_err
            });

            // Assert
            assert!(matches!(dirty_err, LgtmProjectError::DirtyWorktree(_)));
            assert!(!Path::new(&managed.path).exists());
            assert!(project.worktrees.is_empty());

            let final_state = serde_json::to_string_pretty(&project).unwrap();
            fixture.write_artifact(
                "given_a_git_project_exists",
                "should_require_force_for_dirty_managed_worktree_and_remove_when_forced",
                &initial_state,
                &final_state,
            );
        }
    }
}
