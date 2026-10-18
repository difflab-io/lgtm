use crate::support;
use lgtm_backend::{models::*, services::*};
use lgtm_db::models::comments;

// tests -----------------------------------------------------------------------

#[tokio::test]
async fn should_preserve_immutable_fields_in_client_owned_transactions() {
    for (service, _, _) in support::databases().await {
        let db = service.database();
        let review = ReviewService::new(db.clone(), service.auth().clone());
        let actor = review
            .auth()
            .ensure_human("Alice", "alice@example.com")
            .await
            .unwrap();
        let context = review
            .ensure_context(
                ContextKey::Notes {
                    path: "/committed".into(),
                },
                None,
            )
            .await
            .unwrap();
        let session = review
            .start_session(&context, &actor, CommitContext::default())
            .await
            .unwrap();
        let comment = review
            .add_comment(&session.id, &actor, "Root", Anchor::General)
            .await
            .unwrap();

        // Compose named DB operations in an explicit, client-owned transaction.
        let tx = db.begin().await.unwrap();
        let mut row = comments::Model::from(&comment);
        row.body = "Updated in the same transaction".into();
        let original = row.clone();
        row.author_id = uuid::Uuid::new_v4();
        row.origin_session_id = None;
        row.thread_id = uuid::Uuid::new_v4();
        row.position = 999;
        row.created_at = "cannot-rewrite-created-at".into();
        assert!(tx.comments.update(&row).await.unwrap());
        tx.commit().await.unwrap();
        let saved = review.get_comment(&context.id, &comment.id).await.unwrap();
        assert_eq!(saved.body, row.body);
        assert_eq!(saved.author.name, "Alice");
        let stored = db
            .comments
            .get_by_id(&context.id, &comment.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.author_id, original.author_id);
        assert_eq!(stored.origin_session_id, original.origin_session_id);
        assert_eq!(stored.thread_id, original.thread_id);
        assert_eq!(stored.position, original.position);
        assert_eq!(stored.created_at, original.created_at);

        let tx = db.begin().await.unwrap();
        let nested = tx.begin().await.unwrap();
        row.body = "Nested savepoint".into();
        nested.comments.update(&row).await.unwrap();
        nested.commit().await.unwrap();
        assert_eq!(
            tx.comments
                .get_by_id(&context.id, &comment.id)
                .await
                .unwrap()
                .unwrap()
                .body,
            row.body
        );
        tx.rollback().await.unwrap();
        assert_eq!(
            review
                .get_comment(&context.id, &comment.id)
                .await
                .unwrap()
                .body,
            saved.body
        );

        let tx = db.begin().await.unwrap();
        row.body = "Dropped".into();
        tx.comments.update(&row).await.unwrap();
        drop(tx);
        assert_eq!(
            review
                .get_comment(&context.id, &comment.id)
                .await
                .unwrap()
                .body,
            saved.body
        );

        // A constraint error lets the caller discard earlier work as well.
        let tx = db.begin().await.unwrap();
        row.body = "Discarded".into();
        tx.comments.update(&row).await.unwrap();
        row.visibility = "invalid".into();
        assert!(tx.comments.update(&row).await.is_err());
        tx.rollback().await.unwrap();
        assert_eq!(
            review
                .get_comment(&context.id, &comment.id)
                .await
                .unwrap()
                .body,
            saved.body
        );
        let mut session_row = db.sessions.get_by_id(&session.id).await.unwrap().unwrap();
        let initial_session = session_row.clone();
        session_row.creator_id = uuid::Uuid::new_v4();
        session_row.base_commit = Some("cannot-change-reviewed-base".into());
        session_row.head_commit = Some("cannot-change-reviewed-head".into());
        session_row.created_at = "cannot-change-created-at".into();
        session_row.completed_at = Some("2026-10-16T00:00:00Z".into());
        session_row.completed_by_id = Some(actor.id);
        session_row.outcome = Some("saved".into());
        session_row.request_key = Some("complete".into());
        session_row.request_hash = Some("hash".into());
        assert!(db.sessions.complete(&session_row).await.unwrap());
        let completed = db.sessions.get_by_id(&session.id).await.unwrap().unwrap();
        assert_eq!(completed.state, SessionState::Completed);
        assert_eq!(completed.creator_id, initial_session.creator_id);
        assert_eq!(completed.base_commit, initial_session.base_commit);
        assert_eq!(completed.head_commit, initial_session.head_commit);
        assert_eq!(completed.created_at, initial_session.created_at);
        assert!(!db.sessions.complete(&session_row).await.unwrap());

        let mut thread = db
            .comments
            .get_thread(&context.id, &comment.thread_id)
            .await
            .unwrap()
            .unwrap();
        let original_thread = thread.clone();
        thread.author_id = uuid::Uuid::new_v4();
        thread.origin_session_id = None;
        thread.anchor_kind = "file".into();
        thread.file_path = Some("cannot-change-anchor.rs".into());
        thread.outdated = true;
        assert!(db.comments.update_thread(&thread).await.unwrap());
        let stored = db
            .comments
            .get_thread(&context.id, &comment.thread_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.author_id, original_thread.author_id);
        assert_eq!(stored.origin_session_id, original_thread.origin_session_id);
        assert_eq!(stored.anchor_kind, original_thread.anchor_kind);
        assert_eq!(stored.file_path, original_thread.file_path);
        assert!(stored.outdated);
        db.close().await;
    }
}
