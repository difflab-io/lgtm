use lgtm_backend::{models::*, services::*};
use lgtm_db::types as lgtm_db_types;

use support::forge::*;
mod support;
use support::databases;

async fn actor(db: &ReviewService, name: &str, model: Option<&str>) -> Actor {
    match model {
        Some(model) => db
            .auth()
            .ensure_agent(name, model, "test-provider")
            .await
            .unwrap(),
        None => db
            .auth()
            .ensure_human(name, &format!("{}@example.com", name.to_lowercase()))
            .await
            .unwrap(),
    }
}

fn snapshot(head: &str) -> ForgeSnapshot {
    ForgeSnapshot {
        complete: true,
        head: head.into(),
        base: Some("base".into()),
        threads: vec![RemoteThread {
            id: "thread-1".into(),
            anchor: Anchor::General,
            resolved: false,
            comments: vec![RemoteComment {
                id: "remote-root".into(),
                kind: "review_comment".into(),
                review_id: Some("review-1".into()),
                body: "Live PR root".into(),
                updated_at: now(),
                author: RemoteAuthor {
                    name: "Remote".into(),
                    email: "remote@github.example".into(),
                    account_id: "github/account".into(),
                },
                reply_to: None,
            }],
        }],
    }
}

#[tokio::test]
async fn shared_forge_refresh_attribution_pending_posts_and_failure_contract() {
    for (db, _, _) in databases().await {
        let alice = actor(&db, "Alice", None).await;
        let bot = actor(&db, "Bot", Some("model-one")).await;
        let c = db
            .ensure_context(
                ContextKey::PullRequest {
                    host: "github.com".into(),
                    repository: "acme/repo".into(),
                    number: 42,
                },
                None,
            )
            .await
            .unwrap();
        assert!(
            db.start_session(&c, &alice, CommitContext::default())
                .await
                .is_err()
        );
        let c = db.get_context(&c.key).await.unwrap().unwrap();
        let c = db
            .refresh(&c, &FixtureProvider(snapshot("head")))
            .await
            .unwrap();
        assert_eq!(c.refresh_status, "stub");
        let s = db
            .start_session(
                &c,
                &alice,
                CommitContext {
                    base: Some("base".into()),
                    head: Some("head".into()),
                },
            )
            .await
            .unwrap();
        let local = db
            .add_comment(&s.id, &bot, "An agent authored this", Anchor::General)
            .await
            .unwrap();
        let rendered = db.prepare_post(&s.id, &local.id, "post-1").await.unwrap();
        assert_eq!(rendered.matches("> Generated via model-one").count(), 1);
        db.posting_uncertain(&c.id, &local.id, "timeout")
            .await
            .unwrap();
        assert!(db.prepare_post(&s.id, &local.id, "post-1").await.is_err());
        db.confirm_post(
            &c.id,
            &local.id,
            "post-1",
            PostReceipt {
                comment_id: "agent-comment".into(),
                kind: "review_comment".into(),
                thread_id: Some("agent-thread".into()),
                review_id: Some("review-2".into()),
                posting_account_id: "posting-account".into(),
            },
        )
        .await
        .unwrap();
        let mut remote = snapshot("head");
        remote.threads.push(RemoteThread {
            id: "agent-thread".into(),
            anchor: Anchor::General,
            resolved: false,
            comments: vec![RemoteComment {
                id: "agent-comment".into(),
                kind: "review_comment".into(),
                review_id: Some("review-2".into()),
                body: rendered,
                updated_at: now(),
                author: RemoteAuthor {
                    name: "Posting account".into(),
                    email: "poster@github.example".into(),
                    account_id: "posting-account".into(),
                },
                reply_to: None,
            }],
        });
        let c = db.get_context(&c.key).await.unwrap().unwrap();
        let c = db
            .refresh(&c, &FixtureProvider(remote.clone()))
            .await
            .unwrap();
        let live = db.get_conversation(&c.id, true).await.unwrap();
        let imported = live.comments.iter().find(|x| x.id == local.id).unwrap();
        assert_eq!(imported.author.id, bot.id);
        assert_eq!(imported.body, local.body);
        assert_eq!(imported.origin_session_id, Some(s.id));
        remote.threads[1].comments[0].body = "Edited live by account".into();
        let c = db
            .refresh(&c, &FixtureProvider(remote.clone()))
            .await
            .unwrap();
        assert_eq!(
            db.get_conversation(&c.id, true)
                .await
                .unwrap()
                .comments
                .iter()
                .find(|x| x.id == local.id)
                .unwrap()
                .body,
            "Edited live by account"
        );
        // A local write makes the previous revision stale: do not overwrite it.
        db.reply(&s.id, &local.thread_id, &alice, "Pending local reply")
            .await
            .unwrap();
        assert!(
            db.refresh(&c, &FixtureProvider(remote.clone()))
                .await
                .is_err()
        );
        let c = db.get_context(&c.key).await.unwrap().unwrap();
        let before = db
            .get_conversation(&c.id, true)
            .await
            .unwrap()
            .comments
            .len();
        let mut incomplete = remote.clone();
        incomplete.complete = false;
        assert!(db.refresh(&c, &FixtureProvider(incomplete)).await.is_err());
        assert_eq!(
            db.get_conversation(&c.id, true)
                .await
                .unwrap()
                .comments
                .len(),
            before
        );
        let c = db.get_context(&c.key).await.unwrap().unwrap();
        let c = db
            .refresh(&c, &FixtureProvider(snapshot("new-head")))
            .await
            .unwrap();
        let live = db.get_conversation(&c.id, true).await.unwrap();
        assert_eq!(
            live.comments
                .iter()
                .find(|x| x.id == local.id)
                .unwrap()
                .remote_presence,
            "missing"
        );
        assert!(
            live.comments
                .iter()
                .any(|x| x.body == "Pending local reply")
        );
        assert!(
            db.complete(&c.id, &alice, completion(), "bad-outcome")
                .await
                .is_err()
        );
        let completed = db
            .complete(
                &c.id,
                &alice,
                Completion {
                    outcome: Some("reject".into()),
                    body: None,
                    submit: true,
                },
                "pr-done",
            )
            .await
            .unwrap();
        assert_eq!(completed.submission_status, "unsubmitted");
        assert!(completed.forge_review_id.is_none());
        let c = db.get_context(&c.key).await.unwrap().unwrap();
        assert!(
            db.start_session(&c, &alice, CommitContext::default())
                .await
                .is_err()
        );
        // Refresh once more, then create the next review without copying comments.
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        let c = db.get_context(&c.key).await.unwrap().unwrap();
        let c = db
            .refresh(&c, &FixtureProvider(snapshot("new-head")))
            .await
            .unwrap();
        let next = db
            .start_session(&c, &alice, CommitContext::default())
            .await
            .unwrap();
        assert_ne!(next.id, s.id);
        assert_eq!(
            db.get_conversation(&c.id, true)
                .await
                .unwrap()
                .comments
                .len(),
            before
        );
        db.database().close().await;
    }
}

fn completion() -> Completion {
    Completion {
        outcome: None,
        body: Some("Ready".into()),
        submit: false,
    }
}

struct FailingProvider(std::sync::atomic::AtomicUsize);

impl ReviewForgeProvider for FailingProvider {
    fn is_stub(&self) -> bool {
        true
    }

    async fn fetch(&self, _: &ContextKey) -> lgtm_backend::error::Result<ForgeSnapshot> {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Err(lgtm_backend::error::Error::Invalid(
            "External fetch failed".into(),
        ))
    }

    async fn head(&self, _: &ContextKey) -> lgtm_backend::error::Result<String> {
        unreachable!()
    }
}

#[tokio::test]
async fn client_can_rollback_database_work_on_provider_failure() {
    use lgtm_db::models::{comments, identities};

    for (review, _, _) in databases().await {
        let actor = review
            .auth()
            .ensure_human("Alice", "alice@example.com")
            .await
            .unwrap();
        let context = review
            .ensure_context(
                ContextKey::PullRequest {
                    host: "github.com".into(),
                    repository: "github.com/acme/rollback".into(),
                    number: 1,
                },
                None,
            )
            .await
            .unwrap();
        let current = review
            .refresh(&context, &FixtureProvider(snapshot("head")))
            .await
            .unwrap();
        let session = review
            .start_session(&current, &actor, CommitContext::default())
            .await
            .unwrap();
        let root = review
            .add_comment(&session.id, &actor, "Committed root", Anchor::General)
            .await
            .unwrap();

        let committed_count = review
            .get_conversation(&context.id, true)
            .await
            .unwrap()
            .comments
            .len();
        let tx = review.database().begin().await.unwrap();
        let identity = identities::Model {
            id: id(),
            name: "Temporary".into(),
            kind: lgtm_db_types::IdentityKind::Human,
            email: Some("temporary@example.com".into()),
            model: None,
            provider: None,
            forge_account: String::new(),
        };
        tx.identities.insert(&identity).await.unwrap();
        let mut draft = comments::Model::from(&root);
        draft.id = id();
        draft.author_id = identity.id;
        draft.position = 1;
        draft.body = "Discard on failure".into();
        tx.comments.insert(&draft).await.unwrap();

        let provider = FailingProvider(std::sync::atomic::AtomicUsize::new(0));
        assert!(provider.fetch(&context.key).await.is_err());
        tx.rollback().await.unwrap();
        assert!(
            review
                .database()
                .identities
                .get_by_id(&identity.id)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            review
                .database()
                .comments
                .get_by_id(&context.id, &draft.id)
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            review
                .get_conversation(&context.id, true)
                .await
                .unwrap()
                .comments
                .len(),
            committed_count
        );
        // DB rollback cannot undo work already done in another system.
        assert_eq!(provider.0.load(std::sync::atomic::Ordering::SeqCst), 1);
        review.database().close().await;
    }
}
