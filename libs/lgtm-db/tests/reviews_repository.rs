//! Integration tests for `ReviewsRepository`.

mod common;

use common::setup_db;
use lgtm_db::{DbError, NewLocalReview, ReviewFilter, ReviewKind, ReviewStatus, UpsertForgeReview};
use std::collections::BTreeMap;
use uuid::Uuid;

// fixtures --------------------------------------------------------------------

fn local_input(git_dir: &str, branch: &str) -> NewLocalReview {
    NewLocalReview {
        git_dir: git_dir.to_string(),
        branch: branch.to_string(),
        base_sha: "aaaa111".to_string(),
        head_sha: "bbbb222".to_string(),
    }
}

fn forge_input(pr_number: i64, branch: &str, head_sha: &str) -> UpsertForgeReview {
    UpsertForgeReview {
        git_dir: "/repo/.git".to_string(),
        branch: branch.to_string(),
        repository_url: "https://github.com/difflab-io/lgtm".to_string(),
        pr_number,
        forge_id: Some(format!("PR_node_{pr_number}")),
        base_sha: "aaaa111".to_string(),
        head_sha: head_sha.to_string(),
        status: ReviewStatus::Open,
    }
}

// tests -----------------------------------------------------------------------

mod when_creating_and_updating_local_and_forge_backed_reviews {
    use super::*;

    #[tokio::test]
    async fn should_assign_sequential_numbers_per_git_dir_for_local_reviews() {
        // Arrange
        let ctx = setup_db().await;

        // Act
        let first = ctx
            .db
            .reviews
            .create_local_review(local_input("/repo-a/.git", "feature/diff-241"))
            .await
            .expect("create first review");
        let second = ctx
            .db
            .reviews
            .create_local_review(local_input("/repo-a/.git", "fix/bug_99"))
            .await
            .expect("create second review");
        let other_repo = ctx
            .db
            .reviews
            .create_local_review(local_input("/repo-b/.git", "main"))
            .await
            .expect("create review in other repo");

        // Assert
        assert_eq!(first.number, 1);
        assert_eq!(first.name, "R1-feature-diff-241");
        assert_eq!(first.kind, ReviewKind::Local);
        assert_eq!(first.status, ReviewStatus::Open);
        assert!(first.get_reviewed_files_map().is_empty());
        assert_eq!(second.number, 2);
        assert_eq!(second.name, "R2-fix-bug_99");
        assert_eq!(other_repo.number, 1);
        assert_eq!(other_repo.name, "R1-main");
    }

    #[tokio::test]
    async fn should_insert_and_update_forge_review_idempotently_by_repo_and_pr_number() {
        // Arrange
        let ctx = setup_db().await;
        let created = ctx
            .db
            .reviews
            .upsert_forge_review(forge_input(128, "feature/diff-241", "head-v1"))
            .await
            .expect("insert forge review");

        // Act
        let mut update = forge_input(128, "feature/diff-241", "head-v2");
        update.status = ReviewStatus::Merged;
        let updated = ctx
            .db
            .reviews
            .upsert_forge_review(update)
            .await
            .expect("update forge review");

        // Assert
        assert_eq!(created.number, 1);
        assert_eq!(created.name, "PR128-feature-diff-241");
        assert_eq!(created.kind, ReviewKind::ForgeBacked);
        assert_eq!(created.pr_number, Some(128));
        assert_eq!(created.head_sha, "head-v1");
        assert_eq!(updated.id, created.id);
        assert_eq!(updated.number, 1);
        assert_eq!(updated.head_sha, "head-v2");
        assert_eq!(updated.status, ReviewStatus::Merged);
    }

    #[tokio::test]
    async fn should_update_review_commit_shas_and_lifecycle_status() {
        // Arrange
        let ctx = setup_db().await;
        let review = ctx
            .db
            .reviews
            .create_local_review(local_input("/repo/.git", "feature/diff-241"))
            .await
            .expect("create review");

        // Act
        let with_commits = ctx
            .db
            .reviews
            .update_review_commits(review.id, "base-new", "head-new")
            .await
            .expect("update commits");
        let closed = ctx
            .db
            .reviews
            .update_review_status(review.id, ReviewStatus::Closed)
            .await
            .expect("close review");

        // Assert
        assert_eq!(with_commits.base_sha, "base-new");
        assert_eq!(with_commits.head_sha, "head-new");
        assert_eq!(closed.status, ReviewStatus::Closed);
    }

    #[tokio::test]
    async fn should_reject_review_update_for_missing_review_id() {
        // Arrange
        let ctx = setup_db().await;

        // Act
        let missing = ctx
            .db
            .reviews
            .update_review_status(Uuid::new_v4(), ReviewStatus::Closed)
            .await;

        // Assert
        assert!(matches!(
            missing,
            Err(DbError::NotFound {
                entity: "review",
                ..
            })
        ));
    }
}

mod when_resolving_and_listing_reviews {
    use super::*;

    #[tokio::test]
    async fn should_resolve_review_by_numeric_number_and_r_prefix() {
        // Arrange
        let ctx = setup_db().await;
        let review = ctx
            .db
            .reviews
            .create_local_review(local_input("/repo/.git", "feature/diff-241"))
            .await
            .expect("create review");

        // Act
        let by_num = ctx
            .db
            .reviews
            .get_review_by_selector("/repo/.git", "1")
            .await
            .expect("resolve 1")
            .expect("found");
        let by_r = ctx
            .db
            .reviews
            .get_review_by_selector("/repo/.git", "R1")
            .await
            .expect("resolve R1")
            .expect("found");

        // Assert
        assert_eq!(by_num.id, review.id);
        assert_eq!(by_r.id, review.id);
    }

    #[tokio::test]
    async fn should_resolve_review_by_pr_prefix_and_full_review_name() {
        // Arrange
        let ctx = setup_db().await;
        let pr = ctx
            .db
            .reviews
            .upsert_forge_review(forge_input(128, "feature/diff-241", "head-sha"))
            .await
            .expect("create forge review");

        // Act
        let by_pr = ctx
            .db
            .reviews
            .get_review_by_selector("/repo/.git", "PR128")
            .await
            .expect("resolve PR128")
            .expect("found");
        let by_name = ctx
            .db
            .reviews
            .get_review_by_selector("/repo/.git", "PR128-feature-diff-241")
            .await
            .expect("resolve full name")
            .expect("found");

        // Assert
        assert_eq!(by_pr.id, pr.id);
        assert_eq!(by_name.id, pr.id);
    }

    #[tokio::test]
    async fn should_get_latest_review_by_branch() {
        // Arrange
        let ctx = setup_db().await;
        let _first = ctx
            .db
            .reviews
            .create_local_review(local_input("/repo/.git", "feature/diff-241"))
            .await
            .expect("first review");
        let second = ctx
            .db
            .reviews
            .create_local_review(local_input("/repo/.git", "feature/diff-241"))
            .await
            .expect("second review");

        // Act
        let latest = ctx
            .db
            .reviews
            .get_latest_review_by_branch("/repo/.git", "feature/diff-241")
            .await
            .expect("query latest")
            .expect("latest exists");

        // Assert
        assert_eq!(latest.id, second.id);
        assert_eq!(latest.number, 2);
    }

    #[tokio::test]
    async fn should_filter_reviews_by_kind_status_and_branch() {
        // Arrange
        let ctx = setup_db().await;
        let local = ctx
            .db
            .reviews
            .create_local_review(local_input("/repo/.git", "feature/local"))
            .await
            .expect("create local");
        let forge = ctx
            .db
            .reviews
            .upsert_forge_review(forge_input(42, "feature/remote", "sha"))
            .await
            .expect("create forge");
        ctx.db
            .reviews
            .update_review_status(local.id, ReviewStatus::Closed)
            .await
            .expect("close local");

        // Act
        let open_forge = ctx
            .db
            .reviews
            .get_reviews(
                "/repo/.git",
                ReviewFilter {
                    kind: Some(ReviewKind::ForgeBacked),
                    status: Some(ReviewStatus::Open),
                    branch: Some("feature/remote".to_string()),
                },
            )
            .await
            .expect("list filtered");

        // Assert
        assert_eq!(open_forge.len(), 1);
        assert_eq!(open_forge[0].id, forge.id);
    }
}

mod when_tracking_viewed_file_progress_across_commits {
    use super::*;

    #[tokio::test]
    async fn should_mark_file_reviewed_at_commit_sha() {
        // Arrange
        let ctx = setup_db().await;
        let review = ctx
            .db
            .reviews
            .create_local_review(local_input("/repo/.git", "feature/diff-241"))
            .await
            .expect("create review");

        // Act
        let updated = ctx
            .db
            .reviews
            .mark_file_reviewed(review.id, "src/lib.rs", "sha-1")
            .await
            .expect("mark file reviewed");
        let updated = ctx
            .db
            .reviews
            .mark_file_reviewed(updated.id, "src/client.rs", "sha-1")
            .await
            .expect("mark second file reviewed");

        // Assert
        let map = updated.get_reviewed_files_map();
        assert_eq!(map.get("src/lib.rs").map(String::as_str), Some("sha-1"));
        assert_eq!(map.get("src/client.rs").map(String::as_str), Some("sha-1"));
    }

    #[tokio::test]
    async fn should_unmark_file_reviewed() {
        // Arrange
        let ctx = setup_db().await;
        let review = ctx
            .db
            .reviews
            .create_local_review(local_input("/repo/.git", "feature/diff-241"))
            .await
            .expect("create review");
        ctx.db
            .reviews
            .mark_file_reviewed(review.id, "src/lib.rs", "sha-1")
            .await
            .expect("mark src/lib.rs");
        ctx.db
            .reviews
            .mark_file_reviewed(review.id, "src/client.rs", "sha-1")
            .await
            .expect("mark src/client.rs");

        // Act
        let unmarked = ctx
            .db
            .reviews
            .unmark_file_reviewed(review.id, "src/lib.rs")
            .await
            .expect("unmark src/lib.rs");

        // Assert
        let map = unmarked.get_reviewed_files_map();
        assert!(!map.contains_key("src/lib.rs"));
        assert_eq!(map.get("src/client.rs").map(String::as_str), Some("sha-1"));
    }

    #[tokio::test]
    async fn should_replace_reviewed_files_map_when_invalidating_stale_shas() {
        // Arrange
        let ctx = setup_db().await;
        let review = ctx
            .db
            .reviews
            .create_local_review(local_input("/repo/.git", "feature/diff-241"))
            .await
            .expect("create review");
        ctx.db
            .reviews
            .mark_file_reviewed(review.id, "src/old.rs", "sha-1")
            .await
            .expect("mark old file");

        // Act
        let mut replacement = BTreeMap::new();
        replacement.insert("src/kept.rs".to_string(), "sha-2".to_string());
        let replaced = ctx
            .db
            .reviews
            .update_reviewed_files(review.id, replacement.clone())
            .await
            .expect("replace reviewed_files");

        // Assert
        assert_eq!(replaced.get_reviewed_files_map(), replacement);
    }
}
