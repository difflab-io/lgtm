//! Integration tests for multi-repository `Database::begin`, `commit`, and `rollback` transactions.

mod common;

use common::setup_db;
use lgtm_db::{NewComment, NewLocalReview, ReviewFilter};

// fixtures --------------------------------------------------------------------

fn sample_local_review() -> NewLocalReview {
    NewLocalReview {
        git_dir: "/repo/.git".to_string(),
        branch: "feature/diff-241".to_string(),
        base_sha: "aaaa111".to_string(),
        head_sha: "bbbb222".to_string(),
    }
}

// tests -----------------------------------------------------------------------

mod when_executing_multi_repository_workflows_inside_a_transaction {
    use super::*;

    #[tokio::test]
    async fn should_commit_identity_review_session_and_comment_writes_atomically() {
        // Arrange
        let ctx = setup_db().await;

        // Act
        let tx = ctx.db.begin().await.expect("begin transaction");
        let human = tx
            .identities
            .upsert_human("sid", "sid@example.com", Some("siddharthkapoor"))
            .await
            .expect("upsert human in tx");
        let review = tx
            .reviews
            .create_local_review(sample_local_review())
            .await
            .expect("create review in tx");
        let session = tx
            .sessions
            .get_or_create_draft_session(review.id, human.id, Some("Batch summary".to_string()))
            .await
            .expect("create draft session in tx");
        let comment = tx
            .comments
            .create_comment(NewComment {
                review_id: review.id,
                session_id: session.id,
                author_id: human.id,
                reply_to_id: None,
                file_path: Some("src/lib.rs".to_string()),
                start_line: None,
                line: Some(15),
                commit_sha: Some("bbbb222".to_string()),
                body: "Transactional comment".to_string(),
            })
            .await
            .expect("create comment in tx");
        tx.commit().await.expect("commit transaction");

        // Assert
        assert!(
            ctx.db
                .identities
                .get_identity(human.id)
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            ctx.db
                .reviews
                .get_review(review.id)
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            ctx.db
                .sessions
                .get_session(session.id)
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            ctx.db
                .comments
                .get_comment(comment.id)
                .await
                .unwrap()
                .is_some()
        );
    }

    #[tokio::test]
    async fn should_read_uncommitted_writes_through_transaction_repositories_before_commit() {
        // Arrange
        let ctx = setup_db().await;
        let tx = ctx.db.begin().await.expect("begin transaction");
        let review = tx
            .reviews
            .create_local_review(sample_local_review())
            .await
            .expect("create review in tx");

        // Act
        let inside_tx = tx
            .reviews
            .get_review_by_selector("/repo/.git", "R1")
            .await
            .expect("read inside tx")
            .expect("visible inside tx");
        tx.commit().await.expect("commit transaction");

        // Assert
        assert_eq!(inside_tx.id, review.id);
    }

    #[tokio::test]
    async fn should_rollback_all_writes_when_transaction_explicitly_aborts_or_drops() {
        // Arrange
        let ctx = setup_db().await;
        let tx = ctx.db.begin().await.expect("begin transaction");
        let human = tx
            .identities
            .upsert_human("sid", "sid@example.com", None)
            .await
            .expect("upsert human in tx");
        let review = tx
            .reviews
            .create_local_review(sample_local_review())
            .await
            .expect("create review in tx");
        let session = tx
            .sessions
            .get_or_create_draft_session(review.id, human.id, None)
            .await
            .expect("create draft session in tx");
        let comment = tx
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
                body: "Rolled back comment".to_string(),
            })
            .await
            .expect("create comment in tx");

        // Act
        tx.rollback().await.expect("rollback transaction");

        // Assert
        assert!(
            ctx.db
                .identities
                .get_identity(human.id)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            ctx.db
                .reviews
                .get_reviews("/repo/.git", ReviewFilter::default())
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            ctx.db
                .sessions
                .get_session(session.id)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            ctx.db
                .comments
                .get_comment(comment.id)
                .await
                .unwrap()
                .is_none()
        );
    }
}
