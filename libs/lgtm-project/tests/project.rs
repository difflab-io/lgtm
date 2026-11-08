//! BDD integration tests for `lgtm-project`.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use lgtm_project::{LgtmProjectError, ProjectService, WorktreeSource};
use lgtm_rsutils::add_worktree;
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

mod given_a_git_project_and_worktrees {
    use super::*;

    mod when_resolving_managed_and_external_worktrees {
        use super::*;

        #[test]
        fn should_group_linked_worktrees_under_parent_project_and_classify_ownership() {
            // Arrange
            let tmp = TempDir::new().unwrap();
            let lgtm_home = tmp.path().join(".lgtm");
            let herdr_wt = tmp
                .path()
                .join(".herdr")
                .join("worktrees")
                .join("lgtm")
                .join("feature-herdr");
            let main_repo = init_git_repo(tmp.path(), "lgtm");
            let ext_entry =
                add_worktree(&main_repo, &herdr_wt, "worktree/feature-herdr", None).unwrap();
            let service = ProjectService::new(Some(lgtm_home));

            // Act
            let (project_from_ext, resolved_ext) =
                service.resolve_project(&ext_entry.path).unwrap();
            let managed_wt = service
                .create_worktree(&project_from_ext, "worktree/feature-managed", None)
                .unwrap();

            // Assert
            assert_eq!(
                project_from_ext.root_path,
                main_repo.to_string_lossy().as_ref()
            );
            assert_eq!(project_from_ext.branch.as_deref(), Some("main"));
            let ext_wt = resolved_ext.expect("should return linked worktree");
            assert_eq!(ext_wt.source, WorktreeSource::External);
            assert_eq!(ext_wt.name, "feature-herdr");
            assert_eq!(managed_wt.source, WorktreeSource::Managed);
            assert_eq!(managed_wt.name, "feature-managed");
            assert!(Path::new(&managed_wt.path).is_dir());
        }
    }

    mod when_creating_and_removing_worktrees {
        use super::*;

        #[test]
        fn should_surface_creation_failures_and_only_delete_managed_worktree_storage() {
            // Arrange
            let tmp = TempDir::new().unwrap();
            let lgtm_home = tmp.path().join(".lgtm");
            let main_repo = init_git_repo(tmp.path(), "lgtm");
            let ext_dir = tmp.path().join("external-wt");
            let ext_entry = add_worktree(&main_repo, &ext_dir, "feature/external", None).unwrap();
            let service = ProjectService::new(Some(lgtm_home));
            let (mut project, _) = service.resolve_project(&main_repo).unwrap();
            let managed = service
                .create_worktree(&project, "worktree/feature-clean", None)
                .unwrap();
            project.upsert_worktree(managed.clone());
            fs::write(Path::new(&managed.path).join("dirty.txt"), "uncommitted").unwrap();

            // Act
            let invalid_err = service
                .create_worktree(&project, "bad..branch", None)
                .expect_err("invalid branch should fail");
            let dup_err = service
                .create_worktree(&project, "main", None)
                .expect_err("checked-out branch should fail");
            let dirty_err = service
                .remove_worktree(&project, Path::new(&managed.path), false)
                .expect_err("dirty managed worktree without force should fail");
            service
                .remove_worktree(&project, Path::new(&managed.path), true)
                .unwrap();
            let ext_err = service
                .remove_worktree(&project, &ext_entry.path, true)
                .expect_err("external worktree disk deletion should be rejected");

            // Assert
            assert!(matches!(invalid_err, LgtmProjectError::InvalidBranch(_)));
            assert!(matches!(
                dup_err,
                LgtmProjectError::BranchAlreadyCheckedOut(_)
            ));
            assert!(matches!(dirty_err, LgtmProjectError::DirtyWorktree(_)));
            assert!(!Path::new(&managed.path).exists());
            assert!(matches!(ext_err, LgtmProjectError::ExternalWorktree(_)));
            assert!(ext_entry.path.exists());
        }
    }
}
