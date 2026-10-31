//! Integration tests for `SessionsRepository`.

mod common;

use chrono::Utc;
use common::{seed_human_review_and_draft_session, setup_db};
use lgtm_db::{DbError, ReviewOutcome, SessionState, UpsertForgeSession};

// fixtures --------------------------------------------------------------------

async fn seed_second_agent(db: &lgtm_db::Database) -> lgtm_db::Identity {
    db.identities
        .upsert_agent("claude-sonnet-4-5", "anthropic", "claude-sonnet-4-5")
        .await
        .expect("seed agent identity")
}

// tests -----------------------------------------------------------------------

mod when_managing_draft_review_sessions_per_reviewer {
    use super::*;

    #[tokio::test]
    async fn should_create_draft_session_with_sequential_review_scoped_number() {
        // Arrange
        let ctx = setup_db().await;

        // Act
        let (human, review, session) = seed_human_review_and_draft_session(&ctx.db).await;

        // Assert
        assert_eq!(session.review_id, review.id);
        assert_eq!(session.author_id, human.id);
        assert_eq!(session.number, 1);
        assert_eq!(session.state, SessionState::Draft);
        assert!(session.outcome.is_none());
        assert!(session.published_at.is_none());
    }

    #[tokio::test]
    async fn should_return_existing_draft_session_on_repeated_get_or_create() {
        // Arrange
        let ctx = setup_db().await;
        let (human, review, first) = seed_human_review_and_draft_session(&ctx.db).await;

        // Act
        let second = ctx
            .db
            .sessions
            .get_or_create_draft_session(review.id, human.id, Some("Updated summary".to_string()))
            .await
            .expect("get or create existing draft");

        // Assert
        assert_eq!(second.id, first.id);
        assert_eq!(second.number, 1);
        assert_eq!(second.body.as_deref(), Some("Updated summary"));
    }

    #[tokio::test]
    async fn should_isolate_concurrent_draft_sessions_between_human_and_agent_authors() {
        // Arrange
        let ctx = setup_db().await;
        let (_human, review, human_draft) = seed_human_review_and_draft_session(&ctx.db).await;
        let agent = seed_second_agent(&ctx.db).await;

        // Act
        let agent_draft = ctx
            .db
            .sessions
            .get_or_create_draft_session(review.id, agent.id, Some("Agent draft".to_string()))
            .await
            .expect("create agent draft");
        let all_sessions = ctx
            .db
            .sessions
            .get_sessions_by_review(review.id)
            .await
            .expect("list sessions");

        // Assert
        assert_ne!(agent_draft.id, human_draft.id);
        assert_eq!(agent_draft.number, 2);
        assert_eq!(agent_draft.author_id, agent.id);
        assert_eq!(all_sessions.len(), 2);
    }
}

mod when_publishing_or_importing_review_sessions {
    use super::*;

    #[tokio::test]
    async fn should_publish_draft_session_with_outcome_and_timestamp() {
        // Arrange
        let ctx = setup_db().await;
        let (_, _, draft) = seed_human_review_and_draft_session(&ctx.db).await;

        // Act
        let published = ctx
            .db
            .sessions
            .publish_session(
                draft.id,
                ReviewOutcome::Approve,
                Some("Looks great".to_string()),
                Some("PRR_remote_1".to_string()),
            )
            .await
            .expect("publish session");

        // Assert
        assert_eq!(published.id, draft.id);
        assert_eq!(published.state, SessionState::Published);
        assert_eq!(published.outcome, Some(ReviewOutcome::Approve));
        assert_eq!(published.body.as_deref(), Some("Looks great"));
        assert_eq!(published.forge_id.as_deref(), Some("PRR_remote_1"));
        assert!(published.published_at.is_some());
    }

    #[tokio::test]
    async fn should_allow_creating_new_draft_session_after_previous_draft_is_published() {
        // Arrange
        let ctx = setup_db().await;
        let (human, review, first_draft) = seed_human_review_and_draft_session(&ctx.db).await;
        ctx.db
            .sessions
            .publish_session(first_draft.id, ReviewOutcome::Comment, None, None)
            .await
            .expect("publish first session");

        // Act
        let second_draft = ctx
            .db
            .sessions
            .get_or_create_draft_session(review.id, human.id, None)
            .await
            .expect("create second draft");

        // Assert
        assert_ne!(second_draft.id, first_draft.id);
        assert_eq!(second_draft.number, 2);
        assert_eq!(second_draft.state, SessionState::Draft);
    }

    #[tokio::test]
    async fn should_reject_publishing_already_published_session() {
        // Arrange
        let ctx = setup_db().await;
        let (_, _, draft) = seed_human_review_and_draft_session(&ctx.db).await;
        ctx.db
            .sessions
            .publish_session(draft.id, ReviewOutcome::Reject, None, None)
            .await
            .expect("first publish");

        // Act
        let duplicate = ctx
            .db
            .sessions
            .publish_session(draft.id, ReviewOutcome::Approve, None, None)
            .await;

        // Assert
        assert!(matches!(duplicate, Err(DbError::Conflict(_))));
    }

    #[tokio::test]
    async fn should_upsert_remote_forge_session_by_review_and_forge_id() {
        // Arrange
        let ctx = setup_db().await;
        let (human, review, local_draft) = seed_human_review_and_draft_session(&ctx.db).await;

        // Act
        let paired_draft = ctx
            .db
            .sessions
            .upsert_forge_session(UpsertForgeSession {
                review_id: review.id,
                author_id: human.id,
                forge_id: "PRR_pending_42".to_string(),
                state: SessionState::Draft,
                outcome: None,
                body: Some("Pending on GitHub".to_string()),
                published_at: None,
            })
            .await
            .expect("pair local draft with remote forge_id");
        let published = ctx
            .db
            .sessions
            .upsert_forge_session(UpsertForgeSession {
                review_id: review.id,
                author_id: human.id,
                forge_id: "PRR_pending_42".to_string(),
                state: SessionState::Published,
                outcome: Some(ReviewOutcome::Approve),
                body: Some("Submitted on GitHub".to_string()),
                published_at: Some(Utc::now()),
            })
            .await
            .expect("update remote session to published");

        // Assert
        assert_eq!(paired_draft.id, local_draft.id);
        assert_eq!(paired_draft.forge_id.as_deref(), Some("PRR_pending_42"));
        assert_eq!(published.id, local_draft.id);
        assert_eq!(published.state, SessionState::Published);
        assert_eq!(published.outcome, Some(ReviewOutcome::Approve));
    }
}
