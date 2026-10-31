//! Integration tests for `ReviewService` across local snapshot reviews, unified diff anchors, and remote pull request synchronization.

mod common;

use common::TestContext;
use lgtm_forge::{
    forge::{ForgeReviewComment, ForgeReviewSession},
    models::{IdentityKind, ReviewGroupBy, ReviewOutcome, SessionState},
    services::{CreateCommentInput, UpdateCommentInput},
};

// tests -----------------------------------------------------------------------

mod when_staging_and_committing_in_local_snapshot_mode_vs_working_checkout_mode {
    use super::*;

    #[tokio::test]
    async fn should_stage_and_shadow_commit_into_snapshot_store_without_dirtying_checkout_index() {
        // Arrange
        let ctx = TestContext::setup().await;
        let initial_head = ctx.checkout_head_sha();
        ctx.write_file(
            "src/lib.rs",
            "pub fn alpha() -> i32 {\n    10\n}\n\npub fn beta() -> i32 {\n    2\n}\n",
        );

        // Act
        let staged = ctx
            .service
            .add_paths(&ctx.git_dir, &["src/lib.rs".to_owned()], true)
            .await
            .unwrap();
        let outcome = ctx
            .service
            .commit_changes(&ctx.git_dir, Some("Shadow snapshot"), None, true)
            .await
            .unwrap();

        // Assert
        assert_eq!(staged, vec!["src/lib.rs".to_owned()]);
        assert!(outcome.local);
        assert_ne!(outcome.commit_sha, initial_head);
        assert_eq!(ctx.checkout_head_sha(), initial_head);
        let status = ctx.checkout_status_porcelain();
        assert!(
            status.starts_with(" M src/lib.rs"),
            "working .git/index must remain unstaged, got: {status}"
        );
    }

    #[tokio::test]
    async fn should_record_agent_identity_when_shadow_committing_with_as_agent() {
        // Arrange
        let ctx = TestContext::setup().await;
        ctx.write_file("README.md", "# LGTM\nUpdated by agent.\n");

        // Act
        ctx.service
            .add_paths(&ctx.git_dir, &["README.md".to_owned()], true)
            .await
            .unwrap();
        let outcome = ctx
            .service
            .commit_changes(
                &ctx.git_dir,
                Some("Agent snapshot"),
                Some("claude-sonnet-4-5"),
                true,
            )
            .await
            .unwrap();

        // Assert
        assert_eq!(
            outcome.author.kind,
            IdentityKind::Agent {
                provider: "anthropic".to_owned(),
                model: "claude-sonnet-4-5".to_owned(),
            }
        );
        let saved = ctx
            .database
            .identities
            .get_identity_by_name("claude-sonnet-4-5")
            .await
            .unwrap()
            .expect("agent identity saved in db");
        assert_eq!(saved.provider.as_deref(), Some("anthropic"));
    }

    #[tokio::test]
    async fn should_commit_to_working_checkout_and_mirror_commit_into_snapshot_store_when_local_is_false()
     {
        // Arrange
        let ctx = TestContext::setup().await;
        let initial_head = ctx.checkout_head_sha();
        ctx.write_file("README.md", "# LGTM\nCommitted to working tree.\n");

        // Act
        ctx.service
            .add_paths(&ctx.git_dir, &["README.md".to_owned()], false)
            .await
            .unwrap();
        let outcome = ctx
            .service
            .commit_changes(&ctx.git_dir, Some("Working tree commit"), None, false)
            .await
            .unwrap();

        // Assert
        assert!(!outcome.local);
        assert_ne!(outcome.commit_sha, initial_head);
        assert_eq!(ctx.checkout_head_sha(), outcome.commit_sha);
        let changed = ctx
            .snapshots
            .get_changed_files(&ctx.git_dir, &initial_head, &outcome.commit_sha)
            .unwrap();
        assert_eq!(changed, vec!["README.md".to_owned()]);
    }

    #[tokio::test]
    async fn should_reject_push_when_local_is_true() {
        // Arrange
        let ctx = TestContext::setup().await;

        // Act
        let result = ctx.service.push_changes(&ctx.git_dir, true).await;

        // Assert
        assert!(result.is_err());
    }
}

mod when_creating_refreshing_and_showing_a_local_snapshot_review {
    use super::*;

    #[tokio::test]
    async fn should_create_local_review_and_draft_session_from_worktree_snapshot() {
        // Arrange
        let ctx = TestContext::setup().await;
        let base_sha = ctx.checkout_head_sha();
        ctx.write_file(
            "src/lib.rs",
            "pub fn alpha() -> i32 {\n    1\n}\n\npub fn beta() -> i32 {\n    22\n}\n\npub fn gamma() -> i32 {\n    3\n}\n",
        );

        // Act
        let (review, session) = ctx.service.create_review(&ctx.git_dir, true).await.unwrap();
        let listed = ctx
            .service
            .get_reviews(&ctx.git_dir, true, false)
            .await
            .unwrap();

        // Assert
        assert!(review.context.is_local());
        assert_eq!(review.name, "R1-feature-diff-241");
        assert_eq!(review.base_sha, base_sha);
        assert_ne!(review.head_sha, base_sha);
        assert_eq!(session.state, SessionState::Draft);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, review.id);
    }

    #[tokio::test]
    async fn should_shift_surviving_comment_lines_and_mark_overwritten_lines_outdated_on_local_refresh()
     {
        // Arrange
        let ctx = TestContext::setup().await;
        ctx.write_file(
            "src/lib.rs",
            "pub fn alpha() -> i32 {\n    10\n}\n\npub fn beta() -> i32 {\n    20\n}\n\npub fn gamma() -> i32 {\n    30\n}\n",
        );
        let (review, _) = ctx.service.create_review(&ctx.git_dir, true).await.unwrap();
        let initial_snap_sha = review.head_sha.clone();

        let beta_comment = ctx
            .service
            .create_comment(CreateCommentInput {
                git_dir: ctx.git_dir.clone(),
                review_selector: Some(review.name.clone()),
                body: "Comment on beta body (line 6) that will be overwritten".to_owned(),
                file_path: Some("src/lib.rs".into()),
                start_line: None,
                line: Some(6),
                reply_to: None,
                local: true,
                as_agent: None,
            })
            .await
            .unwrap();
        let gamma_comment = ctx
            .service
            .create_comment(CreateCommentInput {
                git_dir: ctx.git_dir.clone(),
                review_selector: Some(review.name.clone()),
                body: "Comment on gamma signature (line 9) that will shift down by 2 lines"
                    .to_owned(),
                file_path: Some("src/lib.rs".into()),
                start_line: None,
                line: Some(9),
                reply_to: None,
                local: true,
                as_agent: None,
            })
            .await
            .unwrap();

        // Act: insert 2 header lines at the top AND overwrite beta's return value (line 6), leaving gamma intact
        ctx.write_file(
            "src/lib.rs",
            "// Header 1\n// Header 2\npub fn alpha() -> i32 {\n    10\n}\n\npub fn beta() -> i32 {\n    999\n}\n\npub fn gamma() -> i32 {\n    30\n}\n",
        );
        let (refreshed, updated_count) = ctx
            .service
            .refresh_review(&ctx.git_dir, Some(&review.name), true)
            .await
            .unwrap();
        let detail = ctx
            .service
            .get_review_detail(
                &ctx.git_dir,
                Some(&review.name),
                true,
                ReviewGroupBy::File,
                false,
            )
            .await
            .unwrap();

        // Assert
        assert_eq!(updated_count, 2);
        assert_ne!(refreshed.head_sha, initial_snap_sha);

        let updated_beta = detail
            .comments
            .iter()
            .find(|c| c.number == beta_comment.number)
            .unwrap();
        assert!(updated_beta.outdated);
        let beta_anchor = updated_beta.anchor.as_ref().unwrap();
        assert_eq!(beta_anchor.original_line, 6);
        assert_eq!(beta_anchor.original_commit_sha, initial_snap_sha);

        let updated_gamma = detail
            .comments
            .iter()
            .find(|c| c.number == gamma_comment.number)
            .unwrap();
        assert!(!updated_gamma.outdated);
        let gamma_anchor = updated_gamma.anchor.as_ref().unwrap();
        assert_eq!(gamma_anchor.line, 11);
        assert_eq!(gamma_anchor.commit_sha, refreshed.head_sha);
        assert_eq!(gamma_anchor.original_line, 9);
        assert_eq!(gamma_anchor.original_commit_sha, initial_snap_sha);
    }

    #[tokio::test]
    async fn should_extract_inline_diff_hunks_and_filter_unresolved_comments_in_show_review() {
        // Arrange
        let ctx = TestContext::setup().await;
        ctx.write_file(
            "src/lib.rs",
            "pub fn alpha() -> i32 {\n    100\n}\n\npub fn beta() -> i32 {\n    2\n}\n\npub fn gamma() -> i32 {\n    3\n}\n",
        );
        let (review, _) = ctx.service.create_review(&ctx.git_dir, true).await.unwrap();
        let resolved_root = ctx
            .service
            .create_comment(CreateCommentInput {
                git_dir: ctx.git_dir.clone(),
                review_selector: Some(review.name.clone()),
                body: "Resolved thread on line 2".to_owned(),
                file_path: Some("src/lib.rs".into()),
                start_line: None,
                line: Some(2),
                reply_to: None,
                local: true,
                as_agent: None,
            })
            .await
            .unwrap();
        ctx.service
            .update_comment(UpdateCommentInput {
                git_dir: ctx.git_dir.clone(),
                comment_number: resolved_root.number,
                body: None,
                file_path: None,
                start_line: None,
                line: None,
                resolved: Some(true),
                local: true,
                as_agent: None,
            })
            .await
            .unwrap();
        let open_root = ctx
            .service
            .create_comment(CreateCommentInput {
                git_dir: ctx.git_dir.clone(),
                review_selector: Some(review.name.clone()),
                body: "Open thread on line 6".to_owned(),
                file_path: Some("src/lib.rs".into()),
                start_line: None,
                line: Some(6),
                reply_to: None,
                local: true,
                as_agent: None,
            })
            .await
            .unwrap();

        // Act
        let unresolved_detail = ctx
            .service
            .get_review_detail(
                &ctx.git_dir,
                Some(&review.name),
                true,
                ReviewGroupBy::Session,
                true,
            )
            .await
            .unwrap();

        // Assert
        assert_eq!(unresolved_detail.comments.len(), 1);
        assert_eq!(unresolved_detail.comments[0].number, open_root.number);
        assert_eq!(unresolved_detail.diff_hunks.len(), 1);
        assert_eq!(unresolved_detail.diff_hunks[0].file_path, "src/lib.rs");
        assert!(!unresolved_detail.diff_hunks[0].lines.is_empty());
    }

    #[tokio::test]
    async fn should_invalidate_reviewed_files_modified_between_viewed_sha_and_refreshed_head_sha() {
        // Arrange
        let ctx = TestContext::setup().await;
        ctx.write_file(
            "src/lib.rs",
            "pub fn alpha() -> i32 {\n    11\n}\n\npub fn beta() -> i32 {\n    2\n}\n",
        );
        let (review, _) = ctx.service.create_review(&ctx.git_dir, true).await.unwrap();
        ctx.service
            .set_file_reviewed(&ctx.git_dir, Some(&review.name), "src/lib.rs", true, true)
            .await
            .unwrap();
        ctx.service
            .set_file_reviewed(&ctx.git_dir, Some(&review.name), "README.md", true, true)
            .await
            .unwrap();

        // Act: modify src/lib.rs but leave README.md untouched, then refresh
        ctx.write_file(
            "src/lib.rs",
            "pub fn alpha() -> i32 {\n    22\n}\n\npub fn beta() -> i32 {\n    2\n}\n",
        );
        let (refreshed, _) = ctx
            .service
            .refresh_review(&ctx.git_dir, Some(&review.name), true)
            .await
            .unwrap();

        // Assert
        assert!(!refreshed.reviewed_files.contains_key("src/lib.rs"));
        assert_eq!(
            refreshed
                .reviewed_files
                .get("README.md")
                .map(String::as_str),
            Some(review.head_sha.as_str())
        );
    }
}

mod when_drafting_replying_to_resolving_and_deleting_local_comments {
    use super::*;

    #[tokio::test]
    async fn should_validate_diff_line_anchor_and_create_general_inline_and_range_comments() {
        // Arrange
        let ctx = TestContext::setup().await;
        let (review, _) = ctx.service.create_review(&ctx.git_dir, true).await.unwrap();

        // Act
        let general = ctx
            .service
            .create_comment(CreateCommentInput {
                git_dir: ctx.git_dir.clone(),
                review_selector: Some(review.name.clone()),
                body: "General review note".to_owned(),
                file_path: None,
                start_line: None,
                line: None,
                reply_to: None,
                local: true,
                as_agent: None,
            })
            .await
            .unwrap();
        let range = ctx
            .service
            .create_comment(CreateCommentInput {
                git_dir: ctx.git_dir.clone(),
                review_selector: Some(review.name.clone()),
                body: "Multi-line range comment".to_owned(),
                file_path: Some("src/lib.rs".into()),
                start_line: Some(1),
                line: Some(3),
                reply_to: None,
                local: true,
                as_agent: None,
            })
            .await
            .unwrap();
        let out_of_range = ctx
            .service
            .create_comment(CreateCommentInput {
                git_dir: ctx.git_dir.clone(),
                review_selector: Some(review.name.clone()),
                body: "Line 999 does not exist".to_owned(),
                file_path: Some("src/lib.rs".into()),
                start_line: None,
                line: Some(999),
                reply_to: None,
                local: true,
                as_agent: None,
            })
            .await;

        // Assert
        assert!(general.anchor.is_none());
        let range_anchor = range.anchor.unwrap();
        assert_eq!(range_anchor.start_line, Some(1));
        assert_eq!(range_anchor.line, 3);
        assert_eq!(range_anchor.original_start_line, Some(1));
        assert_eq!(range_anchor.original_line, 3);
        assert!(out_of_range.is_err());
    }

    #[tokio::test]
    async fn should_create_reply_resolve_root_thread_and_cascade_delete_replies() {
        // Arrange
        let ctx = TestContext::setup().await;
        let (review, _) = ctx.service.create_review(&ctx.git_dir, true).await.unwrap();
        let root = ctx
            .service
            .create_comment(CreateCommentInput {
                git_dir: ctx.git_dir.clone(),
                review_selector: Some(review.name.clone()),
                body: "Root inline comment".to_owned(),
                file_path: Some("src/lib.rs".into()),
                start_line: None,
                line: Some(2),
                reply_to: None,
                local: true,
                as_agent: None,
            })
            .await
            .unwrap();

        // Act
        let reply = ctx
            .service
            .create_comment(CreateCommentInput {
                git_dir: ctx.git_dir.clone(),
                review_selector: Some(review.name.clone()),
                body: "Agent reply".to_owned(),
                file_path: None,
                start_line: None,
                line: None,
                reply_to: Some(root.number),
                local: true,
                as_agent: Some("claude-sonnet-4-5".into()),
            })
            .await
            .unwrap();
        let resolved_root = ctx
            .service
            .update_comment(UpdateCommentInput {
                git_dir: ctx.git_dir.clone(),
                comment_number: root.number,
                body: None,
                file_path: None,
                start_line: None,
                line: None,
                resolved: Some(true),
                local: true,
                as_agent: Some("claude-sonnet-4-5".into()),
            })
            .await
            .unwrap();
        let deleted_num = ctx
            .service
            .delete_comment(&ctx.git_dir, root.number, true, None)
            .await
            .unwrap();
        let detail = ctx
            .service
            .get_review_detail(
                &ctx.git_dir,
                Some(&review.name),
                true,
                ReviewGroupBy::File,
                false,
            )
            .await
            .unwrap();

        // Assert
        assert_eq!(reply.reply_to_number, Some(root.number));
        assert!(resolved_root.resolved);
        assert_eq!(deleted_num, root.number);
        assert!(detail.comments.is_empty());
    }

    #[tokio::test]
    async fn should_close_local_review_and_publish_draft_session_with_outcome() {
        // Arrange
        let ctx = TestContext::setup().await;
        let (review, _) = ctx.service.create_review(&ctx.git_dir, true).await.unwrap();

        // Act
        let (_, published_session) = ctx
            .service
            .close_review(
                &ctx.git_dir,
                Some(&review.name),
                true,
                ReviewOutcome::Approve,
                Some("Looks great locally".to_owned()),
                None,
            )
            .await
            .unwrap();

        // Assert
        assert_eq!(published_session.state, SessionState::Published);
        assert_eq!(published_session.outcome, Some(ReviewOutcome::Approve));
        assert_eq!(
            published_session.body.as_deref(),
            Some("Looks great locally")
        );
        assert!(ctx.forge.state().submitted_reviews.is_empty());
    }
}

mod when_syncing_a_forge_backed_pull_request_review_through_remote_forge {
    use super::*;

    #[tokio::test]
    async fn should_import_pull_request_sessions_and_comments_into_db_on_create_and_refresh() {
        // Arrange
        let ctx = TestContext::setup().await;
        let head_sha = ctx.checkout_head_sha();
        let bundle = ctx.sample_pr_bundle(
            42,
            &head_sha,
            &head_sha,
            vec![ForgeReviewSession {
                forge_id: "PRR_100".to_owned(),
                author_name: "Grace Hopper".to_owned(),
                author_email: "grace@example.com".to_owned(),
                author_login: "ghopper".to_owned(),
                state: SessionState::Published,
                outcome: Some(ReviewOutcome::Comment),
                body: Some("Initial remote review".to_owned()),
                published_at: None,
            }],
            vec![ForgeReviewComment {
                forge_id: "PRRC_200".to_owned(),
                session_forge_id: "PRR_100".to_owned(),
                reply_to_forge_id: None,
                author_name: "Grace Hopper".to_owned(),
                author_email: "grace@example.com".to_owned(),
                author_login: "ghopper".to_owned(),
                file_path: Some("src/lib.rs".to_owned()),
                start_line: None,
                line: Some(2),
                commit_sha: Some(head_sha.clone()),
                original_start_line: None,
                original_line: Some(2),
                original_commit_sha: Some(head_sha.clone()),
                body: "Consider const fn here".to_owned(),
                resolved: false,
                outdated: false,
            }],
        );
        ctx.forge.set_bundle(bundle);

        // Act
        let (review, draft_session) = ctx
            .service
            .create_review(&ctx.git_dir, false)
            .await
            .unwrap();
        let detail = ctx
            .service
            .get_review_detail(
                &ctx.git_dir,
                Some(&review.name),
                false,
                ReviewGroupBy::File,
                false,
            )
            .await
            .unwrap();

        // Assert
        assert!(!review.context.is_local());
        assert_eq!(review.name, "PR42-feature-diff-241");
        assert_eq!(draft_session.state, SessionState::Draft);
        assert_eq!(detail.sessions.len(), 2);
        assert_eq!(detail.comments.len(), 1);
        assert_eq!(detail.comments[0].forge_id.as_deref(), Some("PRRC_200"));
        assert_eq!(detail.comments[0].body, "Consider const fn here");
    }

    #[tokio::test]
    async fn should_save_draft_comment_locally_and_sync_to_remote_forge_with_agent_disclosure() {
        // Arrange
        let ctx = TestContext::setup().await;
        let head_sha = ctx.checkout_head_sha();
        let bundle = ctx.sample_pr_bundle(42, &head_sha, &head_sha, Vec::new(), Vec::new());
        ctx.forge.set_bundle(bundle);
        let (review, _) = ctx
            .service
            .create_review(&ctx.git_dir, false)
            .await
            .unwrap();

        // Act
        let comment = ctx
            .service
            .create_comment(CreateCommentInput {
                git_dir: ctx.git_dir.clone(),
                review_selector: Some(review.name.clone()),
                body: "Verify bounds check".to_owned(),
                file_path: Some("src/lib.rs".into()),
                start_line: None,
                line: Some(2),
                reply_to: None,
                local: false,
                as_agent: Some("claude-sonnet-4-5".into()),
            })
            .await
            .unwrap();
        let updated = ctx
            .service
            .update_comment(UpdateCommentInput {
                git_dir: ctx.git_dir.clone(),
                comment_number: comment.number,
                body: Some("Updated bounds check note".to_owned()),
                file_path: None,
                start_line: Some(1),
                line: None,
                resolved: Some(true),
                local: false,
                as_agent: Some("claude-sonnet-4-5".into()),
            })
            .await
            .unwrap();
        let deleted_number = ctx
            .service
            .delete_comment(&ctx.git_dir, comment.number, false, None)
            .await
            .unwrap();

        // Assert
        assert!(comment.forge_id.is_some());
        assert_eq!(updated.anchor.as_ref().and_then(|a| a.start_line), Some(1));
        assert_eq!(deleted_number, comment.number);
        let state = ctx.forge.state();
        assert_eq!(state.created_draft_comments.len(), 1);
        let (_, _, ref payload) = state.created_draft_comments[0];
        assert!(
            payload
                .body
                .contains("> Generated by AI agent `anthropic/claude-sonnet-4-5` via LGTM")
        );
        assert_eq!(state.updated_comments.len(), 1);
        assert_eq!(state.deleted_comments.len(), 1);
    }

    #[tokio::test]
    async fn should_flush_unsynced_draft_comments_when_refreshing_or_closing_forge_backed_review() {
        // Arrange
        let ctx = TestContext::setup().await;
        let head_sha = ctx.checkout_head_sha();
        let bundle = ctx.sample_pr_bundle(42, &head_sha, &head_sha, Vec::new(), Vec::new());
        ctx.forge.set_bundle(bundle);
        let (review, _) = ctx
            .service
            .create_review(&ctx.git_dir, false)
            .await
            .unwrap();

        // Simulate transient offline failure during create_comment so forge_id stays None in lgtm-db
        ctx.forge.set_fail_draft_comment_sync(true);
        let unsynced = ctx
            .service
            .create_comment(CreateCommentInput {
                git_dir: ctx.git_dir.clone(),
                review_selector: Some(review.name.clone()),
                body: "Saved locally while offline".to_owned(),
                file_path: Some("src/lib.rs".into()),
                start_line: None,
                line: Some(2),
                reply_to: None,
                local: false,
                as_agent: None,
            })
            .await
            .unwrap();
        assert!(unsynced.forge_id.is_none());

        // Act: restore network and call refresh_review
        ctx.forge.set_fail_draft_comment_sync(false);
        ctx.service
            .refresh_review(&ctx.git_dir, Some(&review.name), false)
            .await
            .unwrap();

        // Simulate another offline draft comment before closing the review
        ctx.forge.set_fail_draft_comment_sync(true);
        let second_unsynced = ctx
            .service
            .create_comment(CreateCommentInput {
                git_dir: ctx.git_dir.clone(),
                review_selector: Some(review.name.clone()),
                body: "Second offline draft flushed on close".to_owned(),
                file_path: Some("src/lib.rs".into()),
                start_line: None,
                line: Some(3),
                reply_to: None,
                local: false,
                as_agent: None,
            })
            .await
            .unwrap();
        assert!(second_unsynced.forge_id.is_none());

        ctx.forge.set_fail_draft_comment_sync(false);
        ctx.service
            .close_review(
                &ctx.git_dir,
                Some(&review.name),
                false,
                ReviewOutcome::Comment,
                Some("Flushed on close".to_owned()),
                None,
            )
            .await
            .unwrap();
        let detail = ctx
            .service
            .get_review_detail(
                &ctx.git_dir,
                Some(&review.name),
                false,
                ReviewGroupBy::File,
                false,
            )
            .await
            .unwrap();

        // Assert
        assert_eq!(ctx.forge.state().created_draft_comments.len(), 2);
        assert_eq!(ctx.forge.state().submitted_reviews.len(), 1);
        assert!(detail.comments.iter().all(|c| c.forge_id.is_some()));
    }

    #[tokio::test]
    async fn should_sync_viewed_file_state_and_submit_pending_review_to_remote_forge_on_close() {
        // Arrange
        let ctx = TestContext::setup().await;
        let head_sha = ctx.checkout_head_sha();
        let bundle = ctx.sample_pr_bundle(42, &head_sha, &head_sha, Vec::new(), Vec::new());
        ctx.forge.set_bundle(bundle);
        let (review, _) = ctx
            .service
            .create_review(&ctx.git_dir, false)
            .await
            .unwrap();

        // Act
        let viewed = ctx
            .service
            .set_file_reviewed(&ctx.git_dir, Some(&review.name), "src/lib.rs", true, false)
            .await
            .unwrap();
        let (_, published_session) = ctx
            .service
            .close_review(
                &ctx.git_dir,
                Some(&review.name),
                false,
                ReviewOutcome::Approve,
                Some("Ship it".to_owned()),
                Some("claude-sonnet-4-5"),
            )
            .await
            .unwrap();

        // Assert
        assert!(viewed.viewed);
        assert_eq!(published_session.state, SessionState::Published);
        assert_eq!(published_session.outcome, Some(ReviewOutcome::Approve));
        let state = ctx.forge.state();
        assert_eq!(state.viewed_files.len(), 1);
        assert_eq!(
            state.viewed_files[0],
            (
                Some("PR_node_42".to_owned()),
                42,
                "src/lib.rs".to_owned(),
                true
            )
        );
        assert_eq!(state.submitted_reviews.len(), 1);
        let (pr_num, _, outcome, submitted_body) = &state.submitted_reviews[0];
        assert_eq!(*pr_num, 42);
        assert_eq!(*outcome, ReviewOutcome::Approve);
        assert!(
            submitted_body
                .as_deref()
                .unwrap_or_default()
                .contains("> Generated by AI agent `anthropic/claude-sonnet-4-5` via LGTM")
        );
    }
}
