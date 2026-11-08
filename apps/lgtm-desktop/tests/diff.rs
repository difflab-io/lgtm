//! BDD integration tests for `ReviewService` and `DiffService` across `<workspace>/.git`, `~/.lgtm/snapshots/$repoSlug`, and the working tree.

mod common;

use common::{
    DIFF_FIXTURE_STAGES, STAGE_C2_FILES, STAGE_S1_FILES, STAGE_S2_FILES, STAGE_WT_FILES,
    seed_five_stage_workspace,
};
use lgtm_desktop::{
    DiffCommitSource, DiffFileEntry, DiffFileStatus, DiffService, ReviewMode, ReviewService,
};

// tests -----------------------------------------------------------------------

mod given_a_git_workspace_in_local_mode {
    use super::*;

    mod when_listing_commits_and_files_with_shadow_snapshots {
        use super::*;

        #[test]
        fn should_include_working_tree_and_local_snapshot_commits_from_shared_repo_slug() {
            // Arrange
            let seeded = seed_five_stage_workspace();
            let review_service = ReviewService::new(None);
            let diff_service = DiffService::new();

            // Act
            let snapshot = diff_service
                .get_state(
                    &seeded.workspace_root,
                    &seeded.lgtm_home,
                    review_service.get_mode(),
                )
                .unwrap();

            // Assert
            assert!(seeded.repo_slug.starts_with("demo-repo-"));
            assert_eq!(DIFF_FIXTURE_STAGES[0].code, "C1");
            assert_eq!(DIFF_FIXTURE_STAGES[0].files.len(), 3);
            assert_eq!(snapshot.review_mode, ReviewMode::Local);
            assert_eq!(snapshot.branch, "feature/diffs");
            assert_eq!(snapshot.commits.len(), 5);
            assert_eq!(snapshot.commits[0].id, "working_tree");
            assert_eq!(snapshot.commits[0].source, DiffCommitSource::WorkingTree);
            assert_eq!(snapshot.commits[1].id, seeded.s2_sha);
            assert_eq!(snapshot.commits[1].title, DIFF_FIXTURE_STAGES[3].title);
            assert_eq!(snapshot.commits[1].source, DiffCommitSource::LocalSnapshot);
            assert_eq!(snapshot.commits[2].id, seeded.s1_sha);
            assert_eq!(snapshot.commits[2].title, DIFF_FIXTURE_STAGES[2].title);
            assert_eq!(snapshot.commits[2].source, DiffCommitSource::LocalSnapshot);
            assert_eq!(snapshot.commits[3].id, seeded.c2_sha);
            assert_eq!(snapshot.commits[3].title, DIFF_FIXTURE_STAGES[1].title);
            assert_eq!(snapshot.commits[3].source, DiffCommitSource::GitCommit);
            assert_eq!(snapshot.commits[4].id, seeded.c1_sha);
            assert_eq!(snapshot.commits[4].title, DIFF_FIXTURE_STAGES[0].title);
            assert_eq!(snapshot.commits[4].source, DiffCommitSource::GitCommit);

            assert_eq!(
                snapshot.selected_commit_ids,
                vec!["working_tree".to_string()]
            );
            assert_eq!(
                snapshot.files,
                vec![
                    DiffFileEntry {
                        path: "docs/architecture/diffs.md".to_string(),
                        old_path: None,
                        status: DiffFileStatus::Added,
                        additions: 3,
                        deletions: 0,
                    },
                    DiffFileEntry {
                        path: "src/lib.rs".to_string(),
                        old_path: None,
                        status: DiffFileStatus::Modified,
                        additions: 1,
                        deletions: 1,
                    },
                ]
            );
        }

        #[test]
        fn should_diff_across_multiple_selected_commits_spanning_snapshot_and_git_odbs() {
            // Arrange
            let seeded = seed_five_stage_workspace();
            let diff_service = DiffService::new();

            // Act: select S1 + S2 (compares C2 in .git against S2 in ~/.lgtm/snapshots/$repoSlug)
            let s1_to_s2_files = diff_service
                .get_files(
                    &seeded.workspace_root,
                    &seeded.lgtm_home,
                    &[seeded.s1_sha.clone(), seeded.s2_sha.clone()],
                )
                .unwrap();

            // Act: select C2 + S1 + S2 + WT (compares C1 in .git against live Working Tree)
            let c2_to_wt_files = diff_service
                .get_files(
                    &seeded.workspace_root,
                    &seeded.lgtm_home,
                    &[
                        seeded.c2_sha.clone(),
                        seeded.s1_sha.clone(),
                        seeded.s2_sha.clone(),
                        "working_tree".to_string(),
                    ],
                )
                .unwrap();

            // Assert
            assert_eq!(
                s1_to_s2_files,
                vec![
                    DiffFileEntry {
                        path: "src/lib.rs".to_string(),
                        old_path: None,
                        status: DiffFileStatus::Modified,
                        additions: 1,
                        deletions: 1,
                    },
                    DiffFileEntry {
                        path: "src/old.rs".to_string(),
                        old_path: None,
                        status: DiffFileStatus::Deleted,
                        additions: 0,
                        deletions: 2,
                    },
                    DiffFileEntry {
                        path: "src/services/diff.rs".to_string(),
                        old_path: None,
                        status: DiffFileStatus::Added,
                        additions: 5,
                        deletions: 0,
                    },
                ]
            );
            assert_eq!(
                c2_to_wt_files,
                vec![
                    DiffFileEntry {
                        path: "docs/architecture/diffs.md".to_string(),
                        old_path: None,
                        status: DiffFileStatus::Added,
                        additions: 3,
                        deletions: 0,
                    },
                    DiffFileEntry {
                        path: "docs/architecture/index.md".to_string(),
                        old_path: None,
                        status: DiffFileStatus::Modified,
                        additions: 1,
                        deletions: 0,
                    },
                    DiffFileEntry {
                        path: "src/lib.rs".to_string(),
                        old_path: None,
                        status: DiffFileStatus::Modified,
                        additions: 3,
                        deletions: 3,
                    },
                    DiffFileEntry {
                        path: "src/old.rs".to_string(),
                        old_path: None,
                        status: DiffFileStatus::Deleted,
                        additions: 0,
                        deletions: 2,
                    },
                    DiffFileEntry {
                        path: "src/services/diff.rs".to_string(),
                        old_path: None,
                        status: DiffFileStatus::Added,
                        additions: 5,
                        deletions: 0,
                    },
                ]
            );
        }
    }

    mod when_reading_file_diff_for_working_tree_vs_historical_commit {
        use super::*;

        #[test]
        fn should_mark_working_tree_diff_writable_and_historical_commit_diff_readonly() {
            // Arrange
            let seeded = seed_five_stage_workspace();
            let diff_service = DiffService::new();

            // Act
            let wt_diff = diff_service
                .get_file_diff(
                    &seeded.workspace_root,
                    &seeded.lgtm_home,
                    "src/lib.rs",
                    &["working_tree".to_string()],
                )
                .unwrap();
            let s2_diff = diff_service
                .get_file_diff(
                    &seeded.workspace_root,
                    &seeded.lgtm_home,
                    "src/lib.rs",
                    std::slice::from_ref(&seeded.s2_sha),
                )
                .unwrap();

            // Assert
            assert!(!wt_diff.readonly);
            assert_eq!(wt_diff.base_sha, seeded.s2_sha);
            assert_eq!(wt_diff.head_sha, "working_tree");
            assert_eq!(wt_diff.base_content, STAGE_S2_FILES[1].1);
            assert_eq!(wt_diff.head_content, STAGE_WT_FILES[2].1);

            assert!(s2_diff.readonly);
            assert_eq!(s2_diff.base_sha, seeded.s1_sha);
            assert_eq!(s2_diff.head_sha, seeded.s2_sha);
            assert_eq!(s2_diff.base_content, STAGE_S1_FILES[1].1);
            assert_eq!(s2_diff.head_content, STAGE_S2_FILES[1].1);
        }

        #[test]
        fn should_classify_added_modified_deleted_and_renamed_files_with_line_counts() {
            // Arrange
            let seeded = seed_five_stage_workspace();
            let diff_service = DiffService::new();
            let old_doc = seeded.workspace_root.join("docs/architecture/index.md");
            let new_doc = seeded.workspace_root.join("docs/architecture/overview.md");
            std::fs::rename(&old_doc, &new_doc).unwrap();

            // Act
            let s1_files = diff_service
                .get_files(
                    &seeded.workspace_root,
                    &seeded.lgtm_home,
                    std::slice::from_ref(&seeded.s1_sha),
                )
                .unwrap();
            let wt_files = diff_service
                .get_files(
                    &seeded.workspace_root,
                    &seeded.lgtm_home,
                    &["working_tree".to_string()],
                )
                .unwrap();
            let renamed_diff = diff_service
                .get_file_diff(
                    &seeded.workspace_root,
                    &seeded.lgtm_home,
                    "docs/architecture/overview.md",
                    &["working_tree".to_string()],
                )
                .unwrap();

            // Assert
            assert_eq!(
                s1_files,
                vec![
                    DiffFileEntry {
                        path: "src/old.rs".to_string(),
                        old_path: None,
                        status: DiffFileStatus::Deleted,
                        additions: 0,
                        deletions: 2,
                    },
                    DiffFileEntry {
                        path: "src/services/diff.rs".to_string(),
                        old_path: None,
                        status: DiffFileStatus::Added,
                        additions: 4,
                        deletions: 0,
                    },
                ]
            );
            assert!(wt_files.contains(&DiffFileEntry {
                path: "docs/architecture/overview.md".to_string(),
                old_path: Some("docs/architecture/index.md".to_string()),
                status: DiffFileStatus::Renamed,
                additions: 0,
                deletions: 0,
            }));
            assert_eq!(renamed_diff.status, DiffFileStatus::Renamed);
            assert_eq!(
                renamed_diff.old_path.as_deref(),
                Some("docs/architecture/index.md")
            );
            assert_eq!(renamed_diff.base_content, STAGE_C2_FILES[0].1);
            assert_eq!(renamed_diff.head_content, STAGE_C2_FILES[0].1);
        }
    }
}

mod given_a_git_workspace_in_forge_backed_mode {
    use super::*;

    mod when_switching_review_mode {
        use super::*;

        #[test]
        fn should_exclude_working_tree_and_local_snapshots_and_list_git_branch_commits() {
            // Arrange
            let seeded = seed_five_stage_workspace();
            let review_service = ReviewService::new(None);
            let diff_service = DiffService::new();

            // Act
            let mode = review_service.set_mode(ReviewMode::ForgeBacked);
            let snapshot = diff_service
                .get_state(&seeded.workspace_root, &seeded.lgtm_home, mode)
                .unwrap();

            // Assert
            assert_eq!(snapshot.review_mode, ReviewMode::ForgeBacked);
            assert_eq!(snapshot.commits.len(), 2);
            assert_eq!(snapshot.commits[0].id, seeded.c2_sha);
            assert_eq!(snapshot.commits[0].source, DiffCommitSource::GitCommit);
            assert_eq!(snapshot.commits[1].id, seeded.c1_sha);
            assert_eq!(snapshot.commits[1].source, DiffCommitSource::GitCommit);
            assert_eq!(snapshot.selected_commit_ids, vec![seeded.c2_sha]);
            assert_eq!(
                snapshot.files,
                vec![
                    DiffFileEntry {
                        path: "docs/architecture/index.md".to_string(),
                        old_path: None,
                        status: DiffFileStatus::Modified,
                        additions: 1,
                        deletions: 0,
                    },
                    DiffFileEntry {
                        path: "src/lib.rs".to_string(),
                        old_path: None,
                        status: DiffFileStatus::Modified,
                        additions: 1,
                        deletions: 1,
                    },
                ]
            );
        }
    }
}
