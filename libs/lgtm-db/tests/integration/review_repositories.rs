//! Public client usage: multi-repository transactions over notes, local and forge contexts.
use lgtm_db::{
    Database,
    models::{comments, identities, review_contexts, sessions, threads},
    types::{ContextKind, IdentityKind, PostingState, SessionState},
};
use uuid::Uuid;

// fixtures --------------------------------------------------------------------
fn human(name: &str) -> identities::Model {
    identities::Model {
        id: Uuid::new_v4(),
        name: name.into(),
        kind: IdentityKind::Human,
        email: Some(format!("{name}@example.com")),
        model: None,
        provider: None,
        forge_account: String::new(),
    }
}

fn session(context_id: Uuid, creator_id: Uuid) -> sessions::Model {
    sessions::Model {
        id: Uuid::new_v4(),
        context_id,
        creator_id,
        state: SessionState::Active,
        base_commit: None,
        head_commit: None,
        created_at: "2026-10-17T00:00:00Z".into(),
        completed_at: None,
        completed_by_id: None,
        outcome: None,
        body: None,
        forge_review_id: None,
        submission_status: "local".into(),
        submission_error: None,
        request_key: None,
        request_hash: None,
    }
}

fn comment(
    thread: &threads::Model,
    session: &sessions::Model,
    author: Uuid,
    position: i64,
) -> comments::Model {
    comments::Model {
        id: Uuid::new_v4(),
        context_id: thread.context_id,
        thread_id: thread.id,
        origin_session_id: Some(session.id),
        author_id: author,
        body: format!("Comment {position}"),
        position,
        visibility: "draft".into(),
        created_at: "2026-10-17T00:00:00Z".into(),
        updated_at: "2026-10-17T00:00:00Z".into(),
        forge_comment_id: None,
        forge_object_kind: None,
        forge_review_id: None,
        forge_body: None,
        forge_updated_at: None,
        posting_account_id: None,
        posting_state: PostingState::Unposted,
        posting_error: None,
        posting_request_key: None,
        payload_hash: None,
        remote_presence: "local".into(),
    }
}

async fn databases() -> Vec<Database> {
    #[allow(unused_mut)]
    let mut result = vec![Database::connect("sqlite::memory:").await.unwrap()];
    #[cfg(feature = "postgres")]
    {
        use sea_orm::ConnectionTrait;
        let url = std::env::var("LGTM_TEST_POSTGRES_URL")
            .expect("PostgreSQL contracts require an explicit disposable database");
        let raw = sea_orm::Database::connect(&url).await.unwrap();
        let schema = format!("workflow_{}", Uuid::new_v4().simple());
        raw.execute_unprepared(&format!("CREATE SCHEMA {schema}"))
            .await
            .unwrap();
        raw.close().await.unwrap();
        let scoped = format!(
            "{url}{}options=-c%20search_path%3D{schema}",
            if url.contains('?') { '&' } else { '?' }
        );
        result.push(Database::connect(&scoped).await.unwrap());
    }
    result
}

// tests -----------------------------------------------------------------------

#[tokio::test]
async fn should_support_multi_actor_reviews_and_successor_sessions() {
    for db in databases().await {
        for kind in [
            ContextKind::Notes,
            ContextKind::Branch,
            ContextKind::PullRequest,
        ] {
            let context_id = Uuid::new_v4();
            let first = human(&format!("first-{context_id}"));
            let second = human(&format!("second-{context_id}"));
            let agent = identities::Model {
                id: Uuid::new_v4(),
                name: "Review agent".into(),
                kind: IdentityKind::Agent,
                email: None,
                model: Some(context_id.to_string()),
                provider: Some("provider".into()),
                forge_account: String::new(),
            };
            let key = format!("github.com/acme/repo-{context_id}");
            let context = review_contexts::Model {
                id: context_id,
                kind,
                notes_path: (kind == ContextKind::Notes).then(|| format!("/notes/{context_id}")),
                repository: (kind != ContextKind::Notes).then(|| key.clone()),
                branch: (kind == ContextKind::Branch).then(|| "refs/heads/main".into()),
                host: (kind == ContextKind::PullRequest).then(|| "github.com".into()),
                number: (kind == ContextKind::PullRequest).then_some(1),
                revision: 0,
                last_refreshed_at: (kind == ContextKind::PullRequest)
                    .then(|| "2026-10-17T00:00:00Z".into()),
                refreshed_head: (kind == ContextKind::PullRequest).then(|| "head".into()),
                refreshed_base: (kind == ContextKind::PullRequest).then(|| "base".into()),
                refresh_error: None,
                refresh_status: if kind == ContextKind::PullRequest {
                    "live"
                } else {
                    "never"
                }
                .into(),
            };
            let mut review = session(context_id, first.id);
            if kind != ContextKind::Notes {
                review.base_commit = Some("base".into());
                review.head_commit = Some("head".into());
            }
            if kind == ContextKind::PullRequest {
                review.submission_status = "unsubmitted".into();
            }
            let mut thread = threads::Model {
                id: Uuid::new_v4(),
                context_id,
                origin_session_id: Some(review.id),
                author_id: first.id,
                anchor_kind: "general".into(),
                file_path: None,
                side: None,
                reviewed_commit: None,
                start_line: None,
                end_line: None,
                state: "open".into(),
                resolved_by_id: None,
                resolved_at: None,
                updated_by_id: None,
                updated_at: "2026-10-17T00:00:00Z".into(),
                forge_thread_id: None,
                outdated: false,
            };
            let mut root = comment(&thread, &review, first.id, 0);
            let mut reply = comment(&thread, &review, agent.id, 1);
            let tx = db.begin().await.unwrap();
            for actor in [&first, &second, &agent] {
                tx.identities.insert(actor).await.unwrap();
            }
            if kind != ContextKind::Notes {
                tx.workspace
                    .insert_repository(&lgtm_db::models::repositories::Model { key })
                    .await
                    .unwrap();
            }
            tx.review_contexts.insert(&context).await.unwrap();
            tx.sessions.insert(&review).await.unwrap();
            tx.comments.insert_thread(&thread).await.unwrap();
            tx.comments.insert(&root).await.unwrap();
            tx.comments.insert(&reply).await.unwrap();
            tx.commit().await.unwrap();

            // Different identities still share a single active session in this context.
            let mut successor = session(context_id, second.id);
            successor.base_commit = review.base_commit.clone();
            successor.head_commit = review.head_commit.clone();
            successor.submission_status = review.submission_status.clone();
            assert!(
                db.sessions
                    .insert(&session(context_id, first.id))
                    .await
                    .is_err()
            );
            assert!(db.sessions.insert(&successor).await.is_err());
            assert_eq!(
                db.sessions
                    .get_active(&context_id)
                    .await
                    .unwrap()
                    .unwrap()
                    .id,
                review.id
            );
            assert!(
                db.comments
                    .get_by_id(&Uuid::new_v4(), &root.id)
                    .await
                    .unwrap()
                    .is_none()
            );

            let tx = db.begin().await.unwrap();
            tx.review_contexts
                .advance_revision(&context_id, Some(0))
                .await
                .unwrap();
            for row in [&mut root, &mut reply] {
                row.visibility = "published".into();
                tx.comments.update(row).await.unwrap();
            }
            review.completed_at = Some("2026-10-17T00:01:00Z".into());
            review.completed_by_id = Some(first.id);
            review.outcome = Some(
                if kind == ContextKind::PullRequest {
                    "comment"
                } else if kind == ContextKind::Branch {
                    "ready-to-address"
                } else {
                    "saved"
                }
                .into(),
            );
            review.request_key = Some("complete-first".into());
            review.request_hash = Some("hash-first".into());
            assert!(tx.sessions.complete(&review).await.unwrap());
            tx.commit().await.unwrap();
            assert!(!db.sessions.complete(&review).await.unwrap());
            assert!(
                db.comments
                    .get_session_drafts(&review.id)
                    .await
                    .unwrap()
                    .is_empty()
            );

            // A second reviewer addresses the original thread; no conversation is copied.
            let tx = db.begin().await.unwrap();
            if kind == ContextKind::PullRequest {
                let mut refreshed = context.clone();
                refreshed.last_refreshed_at = Some("2026-10-17T00:02:00Z".into());
                tx.review_contexts.update_refresh(&refreshed).await.unwrap();
            }
            assert!(
                tx.review_contexts
                    .advance_revision(&context_id, None)
                    .await
                    .unwrap()
            );
            tx.sessions.insert(&successor).await.unwrap();
            let response = comment(
                &thread,
                &successor,
                second.id,
                tx.comments.get_next_position(&thread.id).await.unwrap(),
            );
            tx.comments.insert(&response).await.unwrap();
            thread.state = "resolved".into();
            thread.resolved_by_id = Some(second.id);
            thread.resolved_at = Some("2026-10-17T00:02:00Z".into());
            thread.updated_by_id = Some(second.id);
            tx.comments.update_thread(&thread).await.unwrap();
            tx.commit().await.unwrap();

            let stored = db.comments.get_all(&context_id).await.unwrap();
            assert_eq!(stored.len(), 3);
            assert_eq!(stored[0].author_id, first.id);
            assert_eq!(stored[1].author_id, agent.id);
            assert_eq!(stored[2].origin_session_id, Some(successor.id));
            assert_eq!(db.comments.get_threads(&context_id).await.unwrap().len(), 1);
            assert_eq!(
                db.comments
                    .get_thread(&context_id, &thread.id)
                    .await
                    .unwrap()
                    .unwrap()
                    .origin_session_id,
                Some(review.id)
            );

            let mut finished = successor.clone();
            finished.completed_at = Some("2026-10-17T00:03:00Z".into());
            finished.completed_by_id = Some(second.id);
            finished.outcome = review.outcome.clone();
            finished.request_key = Some("complete-second".into());
            finished.request_hash = Some("hash-second".into());
            let tx = db.begin().await.unwrap();
            let mut response = stored[2].clone();
            response.visibility = "published".into();
            tx.comments.update(&response).await.unwrap();
            tx.sessions.complete(&finished).await.unwrap();
            tx.commit().await.unwrap();
            assert!(db.sessions.get_active(&context_id).await.unwrap().is_none());
            assert_eq!(
                db.sessions
                    .get_all(Some(&context_id), false)
                    .await
                    .unwrap()
                    .len(),
                2
            );

            // Invalid discriminated shapes are rejected even through the lower-level client.
            let mut invalid = agent.clone();
            invalid.id = Uuid::new_v4();
            invalid.name.push_str("-invalid");
            invalid.email = Some("agent-must-not-have-email@example.com".into());
            assert!(db.identities.insert(&invalid).await.is_err());
            let mut inconsistent = root.clone();
            inconsistent.posting_error = Some("error on an unposted record".into());
            assert!(db.comments.update(&inconsistent).await.is_err());
        }
        db.close().await;
    }
}
