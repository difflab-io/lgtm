//! BDD integration tests for `lgtm-rsutils` Git path resolution and worktree management.

use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    process::Command,
};

use lgtm_rsutils::{
    add_worktree, atomic_write, checkout_branch, get_current_branch, list_local_branches,
    list_worktrees, normalize_repository_url, parse_github_owner_repo, remove_worktree,
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

mod given_resolve_git_paths {
    use super::*;

    mod when_resolving_linked_worktree_or_empty_path {
        use super::*;

        #[test]
        fn should_resolve_parent_roots_and_reject_empty_input() {
            // Arrange
            let tmp = TempDir::new().unwrap();
            let main_repo = init_git_repo(tmp.path());
            let wt_target = tmp.path().join("worktrees").join("feature-resolve");
            let created = add_worktree(&main_repo, &wt_target, "feature/resolve", None).unwrap();

            // Act
            let resolved = resolve_git_paths(&created.path).unwrap();
            let empty_err = resolve_git_paths(Path::new("   ")).unwrap_err();

            // Assert
            assert_eq!(resolved.main_root, main_repo);
            assert_eq!(resolved.worktree_root, created.path);
            assert_eq!(resolved.common_git_dir, main_repo.join(".git"));
            assert_eq!(empty_err.kind(), ErrorKind::InvalidInput);
        }
    }
}

mod given_get_current_branch {
    use super::*;

    mod when_checked_in_repo_and_non_git_dir {
        use super::*;

        #[test]
        fn should_return_active_branch_or_none() {
            // Arrange
            let tmp = TempDir::new().unwrap();
            let main_repo = init_git_repo(tmp.path());
            let plain_dir = tmp.path().join("plain");
            fs::create_dir_all(&plain_dir).unwrap();

            // Act
            let repo_branch = get_current_branch(&main_repo);
            let plain_branch = get_current_branch(&plain_dir);

            // Assert
            assert_eq!(repo_branch.as_deref(), Some("main"));
            assert_eq!(plain_branch, None);
        }
    }
}

mod given_list_local_branches {
    use super::*;

    mod when_repository_has_multiple_local_branches {
        use super::*;

        #[test]
        fn should_return_sorted_branch_names() {
            // Arrange
            let tmp = TempDir::new().unwrap();
            let main_repo = init_git_repo(tmp.path());
            checkout_branch(&main_repo, "feature/alpha", true).unwrap();
            checkout_branch(&main_repo, "main", false).unwrap();

            // Act
            let branches = list_local_branches(&main_repo);

            // Assert
            assert_eq!(
                branches,
                vec!["feature/alpha".to_owned(), "main".to_owned()]
            );
        }
    }
}

mod given_checkout_branch {
    use super::*;

    mod when_creating_switching_or_passing_invalid_name {
        use super::*;

        #[test]
        fn should_switch_branches_and_reject_invalid_branch_names() {
            // Arrange
            let tmp = TempDir::new().unwrap();
            let main_repo = init_git_repo(tmp.path());

            // Act
            let created = checkout_branch(&main_repo, "feature/beta", true).unwrap();
            let switched = checkout_branch(&main_repo, "main", false).unwrap();
            let invalid_err = checkout_branch(&main_repo, "bad branch", true).unwrap_err();

            // Assert
            assert_eq!(created, "feature/beta");
            assert_eq!(switched, "main");
            assert_eq!(invalid_err.kind(), ErrorKind::InvalidInput);
        }
    }
}

mod given_list_worktrees {
    use super::*;

    mod when_main_repo_has_linked_worktree {
        use super::*;

        #[test]
        fn should_return_main_checkout_first_then_linked_worktrees() {
            // Arrange
            let tmp = TempDir::new().unwrap();
            let main_repo = init_git_repo(tmp.path());
            let wt_target = tmp.path().join("worktrees").join("feature-list");
            add_worktree(&main_repo, &wt_target, "feature/list", None).unwrap();

            // Act
            let worktrees = list_worktrees(&main_repo).unwrap();

            // Assert
            assert_eq!(worktrees.len(), 2);
            assert!(worktrees[0].is_main);
            assert_eq!(worktrees[0].branch.as_deref(), Some("main"));
            assert!(!worktrees[1].is_main);
            assert_eq!(worktrees[1].branch.as_deref(), Some("feature/list"));
        }
    }
}

mod given_add_worktree {
    use super::*;

    mod when_creating_worktrees {
        use super::*;

        #[test]
        fn should_create_linked_worktree_and_reject_duplicate_branch_or_existing_path() {
            // Arrange
            let tmp = TempDir::new().unwrap();
            let main_repo = init_git_repo(tmp.path());
            let wt_target = tmp.path().join("worktrees").join("feature-one");

            // Act
            let created = add_worktree(&main_repo, &wt_target, "feature/one", None).unwrap();
            let dup_branch = add_worktree(
                &main_repo,
                &tmp.path().join("worktrees").join("feature-one-dup"),
                "feature/one",
                None,
            )
            .unwrap_err();
            let existing_path =
                add_worktree(&main_repo, &wt_target, "feature/two", None).unwrap_err();

            // Assert
            assert_eq!(created.branch.as_deref(), Some("feature/one"));
            assert_eq!(dup_branch.kind(), ErrorKind::AlreadyExists);
            assert_eq!(existing_path.kind(), ErrorKind::AlreadyExists);
        }
    }
}

mod given_remove_worktree {
    use super::*;

    mod when_worktree_has_uncommitted_changes {
        use super::*;

        #[test]
        fn should_require_force_before_removing_directory() {
            // Arrange
            let tmp = TempDir::new().unwrap();
            let main_repo = init_git_repo(tmp.path());
            let wt_target = tmp.path().join("worktrees").join("feature-dirty");
            let created = add_worktree(&main_repo, &wt_target, "feature/dirty", None).unwrap();
            atomic_write(&created.path.join("untracked.txt"), b"dirty").unwrap();

            // Act
            let dirty_err = remove_worktree(&main_repo, &created.path, false)
                .expect_err("dirty worktree removal without force should fail");
            remove_worktree(&main_repo, &created.path, true).unwrap();

            // Assert
            assert_eq!(dirty_err.kind(), ErrorKind::WouldBlock);
            assert!(!created.path.exists());
        }
    }
}

mod given_normalize_repository_url {
    use super::*;

    mod when_given_ssh_and_https_remotes {
        use super::*;

        #[test]
        fn should_return_canonical_https_urls() {
            // Arrange
            let ssh_url = "git@github.com:difflab-io/lgtm.git";
            let https_url = "https://github.com/difflab-io/lgtm.git/";

            // Act
            let normalized_ssh = normalize_repository_url(ssh_url).unwrap();
            let normalized_https = normalize_repository_url(https_url).unwrap();

            // Assert
            assert_eq!(normalized_ssh, "https://github.com/difflab-io/lgtm");
            assert_eq!(normalized_https, "https://github.com/difflab-io/lgtm");
        }
    }

    mod when_given_empty_or_invalid_remotes {
        use super::*;

        #[test]
        fn should_return_invalid_input() {
            // Arrange
            let empty = "   ";
            let unsupported = "ftp://github.com/difflab-io/lgtm";

            // Act
            let empty_err = normalize_repository_url(empty).unwrap_err();
            let scheme_err = normalize_repository_url(unsupported).unwrap_err();

            // Assert
            assert_eq!(empty_err.kind(), ErrorKind::InvalidInput);
            assert_eq!(scheme_err.kind(), ErrorKind::InvalidInput);
        }
    }
}

mod given_parse_github_owner_repo {
    use super::*;

    mod when_given_valid_remote_url {
        use super::*;

        #[test]
        fn should_extract_owner_and_repo() {
            // Arrange
            let ssh_url = "git@github.com:difflab-io/lgtm.git";

            // Act
            let owner_repo = parse_github_owner_repo(ssh_url).unwrap();

            // Assert
            assert_eq!(owner_repo, ("difflab-io".to_owned(), "lgtm".to_owned()));
        }
    }

    mod when_given_missing_repo_segment {
        use super::*;

        #[test]
        fn should_return_invalid_input() {
            // Arrange
            let missing_repo = "https://github.com/difflab-io";

            // Act
            let err = parse_github_owner_repo(missing_repo).unwrap_err();

            // Assert
            assert_eq!(err.kind(), ErrorKind::InvalidInput);
        }
    }
}
