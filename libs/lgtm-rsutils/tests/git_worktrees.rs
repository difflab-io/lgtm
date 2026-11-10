//! BDD integration tests for `lgtm-rsutils` Git path resolution and worktree management.

use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    process::Command,
};

use lgtm_rsutils::{
    add_worktree, atomic_write, get_current_branch, list_worktrees, remove_worktree,
    resolve_git_paths,
};
use tempfile::TempDir;

// fixtures --------------------------------------------------------------------

fn init_git_repo(dir: &Path) -> PathBuf {
    let repo = dir.join("sample-repo");
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
    fs::write(repo.join("README.md"), "# Sample\n").unwrap();
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

mod given_a_git_repository {
    use super::*;

    mod when_resolving_paths_and_managing_worktrees {
        use super::*;

        #[test]
        fn should_resolve_linked_worktree_to_parent_main_root_and_common_git_dir() {
            // Arrange
            let tmp = TempDir::new().unwrap();
            let main_repo = init_git_repo(tmp.path());
            let wt_target = tmp.path().join("worktrees").join("feature-one");

            // Act
            let created = add_worktree(&main_repo, &wt_target, "feature/one", None).unwrap();
            let resolved = resolve_git_paths(&created.path).unwrap();
            let worktrees = list_worktrees(&main_repo).unwrap();

            // Assert
            assert_eq!(resolved.main_root, main_repo);
            assert_eq!(resolved.worktree_root, created.path);
            assert_eq!(resolved.common_git_dir, main_repo.join(".git"));
            assert_eq!(
                get_current_branch(&created.path).as_deref(),
                Some("feature/one")
            );
            assert_eq!(worktrees.len(), 2);
            assert!(worktrees[0].is_main);
            assert!(!worktrees[1].is_main);
            assert_eq!(worktrees[1].branch.as_deref(), Some("feature/one"));
        }

        #[test]
        fn should_reject_duplicate_branch_checkout_and_require_force_for_dirty_worktree_removal() {
            // Arrange
            let tmp = TempDir::new().unwrap();
            let main_repo = init_git_repo(tmp.path());
            let wt_target = tmp.path().join("worktrees").join("feature-dirty");
            let created = add_worktree(&main_repo, &wt_target, "feature/dirty", None).unwrap();
            atomic_write(&created.path.join("untracked.txt"), b"dirty").unwrap();
            let duplicate_target = tmp.path().join("worktrees").join("feature-dirty-2");

            // Act
            let dup_err = add_worktree(&main_repo, &duplicate_target, "feature/dirty", None)
                .expect_err("duplicate branch checkout should fail");
            let dirty_err = remove_worktree(&main_repo, &created.path, false)
                .expect_err("dirty worktree removal without force should fail");
            remove_worktree(&main_repo, &created.path, true).unwrap();

            // Assert
            assert_eq!(dup_err.kind(), ErrorKind::AlreadyExists);
            assert_eq!(dirty_err.kind(), ErrorKind::WouldBlock);
            assert!(!created.path.exists());
        }
    }
}
