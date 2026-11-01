//! Integration tests for `CommentsRepository`.

mod common;

use common::{seed_human_review_and_draft_session, setup_db};
use lgtm_db::{
    CommentAnchorShift, DbError, NewComment, NewLocalReview, ReviewOutcome, UpdateComment,
    UpsertForgeComment,
};

// fixtures --------------------------------------------------------------------

fn line_comment(
    review_id: uuid::Uuid,
    session_id: uuid::Uuid,
    author_id: uuid::Uuid,
    file_path: &str,
    start_line: Option<i32>,
    line: i32,
    body: &str,
) -> NewComment {
    NewComment {
        review_id,
        session_id,
        author_id,
        reply_to_id: None,
        file_path: Some(file_path.to_string()),
        start_line,
        line: Some(line),
        commit_sha: Some("2222222".to_string()),
        body: body.to_string(),
    }
}

// tests -----------------------------------------------------------------------

mod when_creating_and_threading_comments {
    use super::*;

    #[tokio::test]
    async fn should_create_general_comment_without_file_or_line_anchors() {
        // Arrange
        let ctx = setup_db().await;
        let (human, review, session) = seed_human_review_and_draft_session(&ctx.db).await;

        // Act
        let general = ctx
            .db
            .comments
            .create_comment(NewComment {
                review_id: review.id,
                session_id: session.id,
                author_id: human.id,
                reply_to_id: None,
                file_path: None,
                start_line: None,
                line: None,
                commit_sha: None,
                body: "Top-level architecture feedback".to_string(),
            })
            .await
            .expect("create general comment");
        let by_num = ctx
            .db
            .comments
            .get_comment_by_number(1)
            .await
            .expect("get by number")
            .expect("found");

        // Assert
        assert_eq!(general.number, 1);
        assert!(general.file_path.is_none());
        assert!(general.start_line.is_none());
        assert!(general.line.is_none());
        assert!(!general.resolved);
        assert!(!general.outdated);
        assert_eq!(by_num.id, general.id);
    }

    #[tokio::test]
    async fn should_create_single_line_and_range_comments_with_original_anchors() {
        // Arrange
        let ctx = setup_db().await;
        let (human, review, session) = seed_human_review_and_draft_session(&ctx.db).await;

        // Act
        let single = ctx
            .db
            .comments
            .create_comment(line_comment(
                review.id,
                session.id,
                human.id,
                "src/lib.rs",
                None,
                24,
                "Single line comment",
            ))
            .await
            .expect("create single line comment");
        let range = ctx
            .db
            .comments
            .create_comment(line_comment(
                review.id,
                session.id,
                human.id,
                "src/client.rs",
                Some(10),
                18,
                "Range comment",
            ))
            .await
            .expect("create range comment");

        // Assert
        assert_eq!(single.number, 1);
        assert_eq!(single.line, Some(24));
        assert_eq!(single.original_line, Some(24));
        assert_eq!(single.original_commit_sha.as_deref(), Some("2222222"));
        assert_eq!(range.number, 2);
        assert_eq!(range.start_line, Some(10));
        assert_eq!(range.line, Some(18));
        assert_eq!(range.original_start_line, Some(10));
        assert_eq!(range.original_line, Some(18));
    }

    #[tokio::test]
    async fn should_create_reply_attached_to_root_comment_in_same_review() {
        // Arrange
        let ctx = setup_db().await;
        let (human, review, session) = seed_human_review_and_draft_session(&ctx.db).await;
        let root = ctx
            .db
            .comments
            .create_comment(line_comment(
                review.id,
                session.id,
                human.id,
                "src/lib.rs",
                None,
                12,
                "Root thread comment",
            ))
            .await
            .expect("create root");

        // Act
        let reply = ctx
            .db
            .comments
            .create_comment(NewComment {
                review_id: review.id,
                session_id: session.id,
                author_id: human.id,
                reply_to_id: Some(root.id),
                file_path: Some("src/lib.rs".to_string()),
                start_line: None,
                line: Some(12),
                commit_sha: Some("2222222".to_string()),
                body: "Follow-up reply".to_string(),
            })
            .await
            .expect("create reply");

        // Assert
        assert_eq!(reply.reply_to_id, Some(root.id));
        assert_eq!(reply.number, 2);
    }

    #[tokio::test]
    async fn should_reject_reply_to_another_reply_or_comment_in_different_review() {
        // Arrange
        let ctx = setup_db().await;
        let (human, review, session) = seed_human_review_and_draft_session(&ctx.db).await;
        let root = ctx
            .db
            .comments
            .create_comment(line_comment(
                review.id,
                session.id,
                human.id,
                "src/lib.rs",
                None,
                12,
                "Root comment",
            ))
            .await
            .expect("create root");
        let reply = ctx
            .db
            .comments
            .create_comment(NewComment {
                review_id: review.id,
                session_id: session.id,
                author_id: human.id,
                reply_to_id: Some(root.id),
                file_path: Some("src/lib.rs".to_string()),
                start_line: None,
                line: Some(12),
                commit_sha: Some("2222222".to_string()),
                body: "First reply".to_string(),
            })
            .await
            .expect("create reply");
        let other_review = ctx
            .db
            .reviews
            .create_local_review(NewLocalReview {
                git_dir: "/repo/.git".to_string(),
                branch: "other".to_string(),
                base_sha: "111".to_string(),
                head_sha: "222".to_string(),
            })
            .await
            .expect("other review");
        let other_session = ctx
            .db
            .sessions
            .get_or_create_draft_session(other_review.id, human.id, None)
            .await
            .expect("other session");

        // Act
        let nested_reply = ctx
            .db
            .comments
            .create_comment(NewComment {
                review_id: review.id,
                session_id: session.id,
                author_id: human.id,
                reply_to_id: Some(reply.id),
                file_path: Some("src/lib.rs".to_string()),
                start_line: None,
                line: Some(12),
                commit_sha: Some("2222222".to_string()),
                body: "Nested reply".to_string(),
            })
            .await;
        let cross_review_reply = ctx
            .db
            .comments
            .create_comment(NewComment {
                review_id: other_review.id,
                session_id: other_session.id,
                author_id: human.id,
                reply_to_id: Some(root.id),
                file_path: None,
                start_line: None,
                line: None,
                commit_sha: None,
                body: "Cross-review reply".to_string(),
            })
            .await;

        // Assert
        assert!(matches!(nested_reply, Err(DbError::InvalidInput(_))));
        assert!(matches!(cross_review_reply, Err(DbError::InvalidInput(_))));
    }

    #[tokio::test]
    async fn should_cascade_delete_replies_when_root_comment_is_deleted() {
        // Arrange
        let ctx = setup_db().await;
        let (human, review, session) = seed_human_review_and_draft_session(&ctx.db).await;
        let root = ctx
            .db
            .comments
            .create_comment(line_comment(
                review.id,
                session.id,
                human.id,
                "src/lib.rs",
                None,
                12,
                "Root comment",
            ))
            .await
            .expect("create root");
        let reply = ctx
            .db
            .comments
            .create_comment(NewComment {
                review_id: review.id,
                session_id: session.id,
                author_id: human.id,
                reply_to_id: Some(root.id),
                file_path: Some("src/lib.rs".to_string()),
                start_line: None,
                line: Some(12),
                commit_sha: Some("2222222".to_string()),
                body: "Reply comment".to_string(),
            })
            .await
            .expect("create reply");

        // Act
        ctx.db
            .comments
            .delete_comment(root.id)
            .await
            .expect("delete root comment");

        // Assert
        assert!(
            ctx.db
                .comments
                .get_comment(root.id)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            ctx.db
                .comments
                .get_comment(reply.id)
                .await
                .unwrap()
                .is_none()
        );
    }
}

mod when_querying_visibility_unsynced_drafts_and_shifting_anchors {
    use super::*;

    #[tokio::test]
    async fn should_return_only_published_comments_when_viewer_id_is_none() {
        // Arrange
        let ctx = setup_db().await;
        let (human, review, human_session) = seed_human_review_and_draft_session(&ctx.db).await;
        let agent = ctx
            .db
            .identities
            .upsert_agent("claude", "anthropic", "claude-sonnet-4-5")
            .await
            .expect("agent");
        let agent_draft_session = ctx
            .db
            .sessions
            .get_or_create_draft_session(review.id, agent.id, None)
            .await
            .expect("agent draft");
        let published_comment = ctx
            .db
            .comments
            .create_comment(line_comment(
                review.id,
                human_session.id,
                human.id,
                "src/lib.rs",
                None,
                5,
                "Published comment",
            ))
            .await
            .expect("create human comment");
        ctx.db
            .sessions
            .publish_session(human_session.id, ReviewOutcome::Comment, None, None)
            .await
            .expect("publish human session");
        ctx.db
            .comments
            .create_comment(line_comment(
                review.id,
                agent_draft_session.id,
                agent.id,
                "src/lib.rs",
                None,
                10,
                "Unpublished agent draft",
            ))
            .await
            .expect("create agent draft comment");

        // Act
        let public_view = ctx
            .db
            .comments
            .get_comments_by_review(review.id, None)
            .await
            .expect("query public comments");

        // Assert
        assert_eq!(public_view.len(), 1);
        assert_eq!(public_view[0].id, published_comment.id);
    }

    #[tokio::test]
    async fn should_include_only_viewers_own_draft_comments_alongside_published_comments() {
        // Arrange
        let ctx = setup_db().await;
        let (human, review, human_session) = seed_human_review_and_draft_session(&ctx.db).await;
        let agent = ctx
            .db
            .identities
            .upsert_agent("claude", "anthropic", "claude-sonnet-4-5")
            .await
            .expect("agent");
        let agent_draft_session = ctx
            .db
            .sessions
            .get_or_create_draft_session(review.id, agent.id, None)
            .await
            .expect("agent draft");
        let published_comment = ctx
            .db
            .comments
            .create_comment(line_comment(
                review.id,
                human_session.id,
                human.id,
                "src/lib.rs",
                None,
                5,
                "Published comment",
            ))
            .await
            .expect("create human comment");
        ctx.db
            .sessions
            .publish_session(human_session.id, ReviewOutcome::Comment, None, None)
            .await
            .expect("publish human session");
        let new_human_draft_session = ctx
            .db
            .sessions
            .get_or_create_draft_session(review.id, human.id, None)
            .await
            .expect("new human draft");
        let human_draft_comment = ctx
            .db
            .comments
            .create_comment(line_comment(
                review.id,
                new_human_draft_session.id,
                human.id,
                "src/lib.rs",
                None,
                8,
                "Human private draft",
            ))
            .await
            .expect("create human draft comment");
        ctx.db
            .comments
            .create_comment(line_comment(
                review.id,
                agent_draft_session.id,
                agent.id,
                "src/lib.rs",
                None,
                10,
                "Agent private draft",
            ))
            .await
            .expect("create agent draft comment");

        // Act
        let human_view = ctx
            .db
            .comments
            .get_comments_by_review(review.id, Some(human.id))
            .await
            .expect("query human viewer comments");

        // Assert
        assert_eq!(human_view.len(), 2);
        assert_eq!(human_view[0].id, published_comment.id);
        assert_eq!(human_view[1].id, human_draft_comment.id);
    }

    #[tokio::test]
    async fn should_return_unsynced_comments_where_forge_id_is_null() {
        // Arrange
        let ctx = setup_db().await;
        let (human, review, session) = seed_human_review_and_draft_session(&ctx.db).await;
        let synced = ctx
            .db
            .comments
            .create_comment(line_comment(
                review.id,
                session.id,
                human.id,
                "src/lib.rs",
                None,
                10,
                "Already synced",
            ))
            .await
            .expect("create synced");
        ctx.db
            .comments
            .update_comment(
                synced.id,
                UpdateComment {
                    forge_id: Some("PRRC_1".to_string()),
                    ..Default::default()
                },
            )
            .await
            .expect("set forge_id");
        let unsynced = ctx
            .db
            .comments
            .create_comment(line_comment(
                review.id,
                session.id,
                human.id,
                "src/lib.rs",
                None,
                20,
                "Pending remote sync",
            ))
            .await
            .expect("create unsynced");

        // Act
        let pending = ctx
            .db
            .comments
            .get_unsynced_comments_by_review(review.id, human.id)
            .await
            .expect("query unsynced");

        // Assert
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, unsynced.id);
    }

    #[tokio::test]
    async fn should_shift_comment_lines_and_mark_outdated_in_batch() {
        // Arrange
        let ctx = setup_db().await;
        let (human, review, session) = seed_human_review_and_draft_session(&ctx.db).await;
        let shifted = ctx
            .db
            .comments
            .create_comment(line_comment(
                review.id,
                session.id,
                human.id,
                "src/lib.rs",
                Some(10),
                15,
                "Shifted range",
            ))
            .await
            .expect("create shifted");
        let invalidated = ctx
            .db
            .comments
            .create_comment(line_comment(
                review.id,
                session.id,
                human.id,
                "src/lib.rs",
                None,
                40,
                "Invalidated line",
            ))
            .await
            .expect("create invalidated");

        // Act
        ctx.db
            .comments
            .update_comment_anchors(&[
                CommentAnchorShift {
                    comment_id: shifted.id,
                    file_path: Some("src/extracted.rs".to_string()),
                    start_line: Some(14),
                    line: Some(19),
                    commit_sha: Some("3333333".to_string()),
                    outdated: false,
                },
                CommentAnchorShift {
                    comment_id: invalidated.id,
                    file_path: None,
                    start_line: None,
                    line: Some(40),
                    commit_sha: Some("3333333".to_string()),
                    outdated: true,
                },
            ])
            .await
            .expect("shift anchors");

        // Assert
        let shifted_after = ctx
            .db
            .comments
            .get_comment(shifted.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(shifted_after.file_path.as_deref(), Some("src/extracted.rs"));
        assert_eq!(shifted_after.start_line, Some(14));
        assert_eq!(shifted_after.line, Some(19));
        assert_eq!(shifted_after.commit_sha.as_deref(), Some("3333333"));
        assert_eq!(shifted_after.original_start_line, Some(10));
        assert_eq!(shifted_after.original_line, Some(15));
        assert_eq!(
            shifted_after.original_commit_sha.as_deref(),
            Some("2222222")
        );
        assert!(!shifted_after.outdated);

        let invalidated_after = ctx
            .db
            .comments
            .get_comment(invalidated.id)
            .await
            .unwrap()
            .unwrap();
        assert!(invalidated_after.outdated);
        assert_eq!(invalidated_after.original_line, Some(40));
    }

    #[tokio::test]
    async fn should_upsert_remote_forge_comment_by_review_and_forge_id() {
        // Arrange
        let ctx = setup_db().await;
        let (human, review, session) = seed_human_review_and_draft_session(&ctx.db).await;
        let imported = ctx
            .db
            .comments
            .upsert_forge_comment(UpsertForgeComment {
                review_id: review.id,
                session_id: session.id,
                author_id: human.id,
                reply_to_id: None,
                forge_id: "PRRC_remote_99".to_string(),
                file_path: Some("src/lib.rs".to_string()),
                start_line: None,
                line: Some(30),
                commit_sha: Some("2222222".to_string()),
                original_start_line: None,
                original_line: Some(30),
                original_commit_sha: Some("2222222".to_string()),
                body: "Initial remote body".to_string(),
                resolved: false,
                outdated: false,
            })
            .await
            .expect("import remote comment");

        // Act
        let updated = ctx
            .db
            .comments
            .upsert_forge_comment(UpsertForgeComment {
                review_id: review.id,
                session_id: session.id,
                author_id: human.id,
                reply_to_id: None,
                forge_id: "PRRC_remote_99".to_string(),
                file_path: Some("src/lib.rs".to_string()),
                start_line: None,
                line: Some(32),
                commit_sha: Some("3333333".to_string()),
                original_start_line: None,
                original_line: Some(30),
                original_commit_sha: Some("2222222".to_string()),
                body: "Edited on GitHub".to_string(),
                resolved: true,
                outdated: false,
            })
            .await
            .expect("update remote comment");

        // Assert
        assert_eq!(updated.id, imported.id);
        assert_eq!(updated.number, imported.number);
        assert_eq!(updated.body, "Edited on GitHub");
        assert!(updated.resolved);
        assert_eq!(updated.line, Some(32));
        assert_eq!(updated.original_line, Some(30));
    }
}
