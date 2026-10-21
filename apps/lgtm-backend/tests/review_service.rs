//! Integration tests for review service.
mod common;

mod review_service {
    //! Review service persistence, lifecycle and external failure contracts.
    mod concurrency {
        use lgtm_backend::models::*;
        use lgtm_backend::utils::*;

        use crate::common;

        // tests --------------------------------------------------------------

        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn should_initialize_parallel_connections_with_foreign_keys_enabled() {
            let temp = tempfile::tempdir().unwrap();
            let url = format!(
                "sqlite://{}?mode=rwc",
                temp.path().join("parallel.sqlite").display()
            );
            let mut tasks = vec![];
            for _ in 0..8 {
                let url = url.clone();
                tasks.push(tokio::spawn(
                    async move { common::service(&url).await.unwrap() },
                ));
            }
            for task in tasks {
                task.await.unwrap().database().close().await;
            }
            let db = common::service(&url).await.unwrap();
            let actor = db.auth().ensure_human("A", "a@example.com").await.unwrap();
            let context = db
                .ensure_context(
                    ContextKey::Local {
                        location: LocalLocation::Notes {
                            path: "/notes".into(),
                        },
                    },
                    None,
                )
                .await
                .unwrap();
            let mut tasks = vec![];
            for _ in 0..12 {
                let db = db.clone();
                let c = context.clone();
                let mut actor = actor.clone();
                actor.id = uuid::Uuid::new_v4();
                tasks.push(tokio::spawn(async move {
                    db.start_session(
                        &c,
                        &actor,
                        CommitContext::default(),
                        ComparisonKind::Legacy,
                        None,
                    )
                    .await
                }));
            }
            for task in tasks {
                assert!(task.await.unwrap().is_err());
            }
            assert!(
                db.get_selected_session(&context.id, None, true)
                    .await
                    .is_err()
            );
            db.database().close().await;
        }

        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn should_share_context_keys_and_serialize_session_races_across_pools() {
            for (db, url, _) in common::databases().await {
                let actor = db
                    .auth()
                    .ensure_human("Alice", "alice@example.com")
                    .await
                    .unwrap();
                for key in [
                    ContextKey::Local {
                        location: LocalLocation::Notes {
                            path: "/race-notes".into(),
                        },
                    },
                    ContextKey::Local {
                        location: LocalLocation::Branch {
                            git_dir: "/repos/acme/race".into(),
                            branch: "refs/heads/topic".into(),
                        },
                    },
                    ContextKey::ForgeBacked {
                        repository_url: ForgeRepositoryUrl::parse("https://github.com/acme/race")
                            .unwrap(),
                        number: 42,
                    },
                ] {
                    let mut tasks = vec![];
                    for _ in 0..4 {
                        let url = url.clone();
                        let key = key.clone();
                        tasks.push(tokio::spawn(async move {
                            let other = common::service(&url).await.unwrap();
                            let context = other.ensure_context(key, None).await.unwrap();
                            other.database().close().await;
                            context
                        }));
                    }
                    let context = tasks.remove(0).await.unwrap();
                    for task in tasks {
                        assert_eq!(task.await.unwrap().id, context.id);
                    }
                    let context = if context.key.is_forge_backed() {
                        db.apply_remote(
                            &context,
                            ForgeSnapshot {
                                state: None,
                                complete: true,
                                head: "head".into(),
                                base: None,
                                threads: vec![],
                            },
                            true,
                        )
                        .await
                        .unwrap()
                    } else {
                        context
                    };
                    let mut starts = vec![];
                    for _ in 0..4 {
                        let url = url.clone();
                        let context = context.clone();
                        let actor = actor.clone();
                        starts.push(tokio::spawn(async move {
                            let other = common::service(&url).await.unwrap();
                            let session = other
                                .start_session(
                                    &context,
                                    &actor,
                                    CommitContext::default(),
                                    ComparisonKind::Legacy,
                                    None,
                                )
                                .await
                                .unwrap();
                            other.database().close().await;
                            session
                        }));
                    }
                    let session = starts.remove(0).await.unwrap();
                    for task in starts {
                        assert_eq!(task.await.unwrap().id, session.id);
                    }
                    if !context.key.is_forge_backed() {
                        let a = actor.clone();
                        let c = context.clone();
                        let other = db.clone();
                        let complete = tokio::spawn(async move {
                            other
                                .complete(
                                    &c.id,
                                    &a,
                                    Completion {
                                        outcome: None,
                                        body: None,
                                        submit: false,
                                    },
                                    "finish-race",
                                    None,
                                    None,
                                )
                                .await
                                .unwrap()
                        });
                        let a = actor.clone();
                        let c = context.clone();
                        let other = db.clone();
                        let start = tokio::spawn(async move {
                            other
                                .start_session(
                                    &c,
                                    &a,
                                    CommitContext::default(),
                                    ComparisonKind::Legacy,
                                    None,
                                )
                                .await
                                .unwrap()
                        });
                        assert_eq!(complete.await.unwrap().id, session.id);
                        let raced = start.await.unwrap();
                        let successor = db
                            .start_session(
                                &context,
                                &actor,
                                CommitContext::default(),
                                ComparisonKind::Legacy,
                                None,
                            )
                            .await
                            .unwrap();
                        assert_ne!(successor.id, session.id);
                        assert!(raced.id == session.id || raced.id == successor.id);
                    }
                }
                db.database().close().await;
            }
        }
    }
    mod forge {
        use lgtm_backend::utils::*;
        use lgtm_backend::{models::*, services::*};
        use lgtm_db::types as lgtm_db_types;

        use crate::common::databases;
        use crate::common::forge::*;

        // fixtures -----------------------------------------------------------
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
                state: None,
                complete: true,
                head: head.into(),
                base: Some("base".into()),
                threads: vec![RemoteThread {
                    observation: None,
                    id: "thread-1".into(),
                    anchor: Anchor::General,
                    resolved: false,
                    comments: vec![RemoteComment {
                        pending: false,
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

        // tests --------------------------------------------------------------

        #[tokio::test]
        async fn should_reconcile_forge_refreshes_and_preserve_attribution_on_post_failures() {
            for (db, _, _) in databases().await {
                let alice = actor(&db, "Alice", None).await;
                let bot = actor(&db, "Bot", Some("model-one")).await;
                let c = db
                    .ensure_context(
                        ContextKey::ForgeBacked {
                            repository_url: ForgeRepositoryUrl::parse(
                                "https://github.com/acme/repo",
                            )
                            .unwrap(),
                            number: 42,
                        },
                        None,
                    )
                    .await
                    .unwrap();
                assert!(
                    db.start_session(
                        &c,
                        &alice,
                        CommitContext::default(),
                        ComparisonKind::Legacy,
                        None
                    )
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
                        ComparisonKind::Legacy,
                        None,
                    )
                    .await
                    .unwrap();
                let local = db
                    .add_comment(
                        &s.id,
                        &bot,
                        "An agent authored this",
                        Anchor::General,
                        None,
                        None,
                    )
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
                    observation: None,
                    id: "agent-thread".into(),
                    anchor: Anchor::General,
                    resolved: false,
                    comments: vec![RemoteComment {
                        pending: false,
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
                db.reply(&s.id, &local.thread_id, &alice, "Pending local reply", None)
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
                assert!(!live.comments.iter().any(|x| x.id == local.id));
                assert!(db.get_comment(&c.id, &local.id).await.is_err());
                assert!(
                    live.comments
                        .iter()
                        .any(|x| x.body == "Pending local reply")
                );
                assert!(
                    db.complete(&c.id, &alice, completion(), "bad-outcome", None, None)
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
                        None,
                        None,
                    )
                    .await
                    .unwrap();
                assert_eq!(completed.submission_status, "unsubmitted");
                assert!(completed.forge_review_id.is_none());
                let c = db.get_context(&c.key).await.unwrap().unwrap();
                assert!(
                    db.start_session(
                        &c,
                        &alice,
                        CommitContext::default(),
                        ComparisonKind::Legacy,
                        None
                    )
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
                    .start_session(
                        &c,
                        &alice,
                        CommitContext::default(),
                        ComparisonKind::Legacy,
                        None,
                    )
                    .await
                    .unwrap();
                assert_ne!(next.id, s.id);
                assert_eq!(
                    db.get_conversation(&c.id, true)
                        .await
                        .unwrap()
                        .comments
                        .len(),
                    before - 1
                );
                db.database().close().await;
            }
        }

        #[tokio::test]
        async fn should_rollback_database_work_when_provider_fails() {
            use lgtm_db::models::{comments, identities};

            for (review, _, _) in databases().await {
                let actor = review
                    .auth()
                    .ensure_human("Alice", "alice@example.com")
                    .await
                    .unwrap();
                let context = review
                    .ensure_context(
                        ContextKey::ForgeBacked {
                            repository_url: ForgeRepositoryUrl::parse(
                                "https://github.com/acme/rollback",
                            )
                            .unwrap(),
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
                    .start_session(
                        &current,
                        &actor,
                        CommitContext::default(),
                        ComparisonKind::Legacy,
                        None,
                    )
                    .await
                    .unwrap();
                let root = review
                    .add_comment(
                        &session.id,
                        &actor,
                        "Committed root",
                        Anchor::General,
                        None,
                        None,
                    )
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
                    id: uuid::Uuid::new_v4(),
                    name: "Temporary".into(),
                    kind: lgtm_db_types::IdentityKind::Human,
                    email: Some("temporary@example.com".into()),
                    model: None,
                    provider: None,
                    forge_account: String::new(),
                };
                tx.identities.insert(&identity).await.unwrap();
                let mut draft = comments::Model::from(&root);
                draft.id = uuid::Uuid::new_v4();
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
    }
    mod lifecycle {
        use lgtm_backend::{models::*, services::*};

        use crate::common;
        use crate::common::databases;

        // fixtures -----------------------------------------------------------
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

        fn completion() -> Completion {
            Completion {
                outcome: None,
                body: Some("Ready".into()),
                submit: false,
            }
        }

        fn notes(path: &str) -> ContextKey {
            ContextKey::Local {
                location: LocalLocation::Notes { path: path.into() },
            }
        }

        // tests --------------------------------------------------------------

        #[tokio::test]
        async fn should_preserve_conversation_across_completed_and_successor_sessions() {
            for (db, url, temp) in databases().await {
                let alice = actor(&db, "Alice", None).await;
                let bot = actor(&db, "Bot", Some("model-one")).await;
                let path = temp.path().join("notes").to_str().unwrap().to_string();
                let context = db.ensure_context(notes(&path), Some(&path)).await.unwrap();
                let mut starts = vec![];
                for _ in 0..10 {
                    let db = db.clone();
                    let c = context.clone();
                    let a = alice.clone();
                    starts.push(tokio::spawn(async move {
                        db.start_session(
                            &c,
                            &a,
                            CommitContext::default(),
                            ComparisonKind::Legacy,
                            None,
                        )
                        .await
                        .unwrap()
                    }));
                }
                let first = starts.remove(0).await.unwrap();
                for handle in starts {
                    assert_eq!(first.id, handle.await.unwrap().id);
                }
                let c = db
                    .add_comment(
                        &first.id,
                        &alice,
                        "**Keep this Markdown**",
                        Anchor::File {
                            path: "src/lib.rs".into(),
                        },
                        None,
                        None,
                    )
                    .await
                    .unwrap();
                let reply = db
                    .reply(&first.id, &c.thread_id, &bot, "Agent reply", None)
                    .await
                    .unwrap();
                assert_eq!(reply.position, 1);
                assert!(
                    db.get_conversation(&context.id, false)
                        .await
                        .unwrap()
                        .comments
                        .is_empty()
                );
                let closed = db
                    .complete(
                        &context.id,
                        &alice,
                        completion(),
                        "complete-one",
                        None,
                        None,
                    )
                    .await
                    .unwrap();
                assert_eq!(
                    closed.completion().map(|c| c.outcome.as_str()),
                    Some("saved")
                );
                assert!(
                    db.add_comment(&first.id, &alice, "closed", Anchor::General, None, None)
                        .await
                        .is_err()
                );
                let next = db
                    .start_session(
                        &context,
                        &bot,
                        CommitContext::default(),
                        ComparisonKind::Legacy,
                        None,
                    )
                    .await
                    .unwrap();
                assert_ne!(next.id, first.id);
                assert_eq!(
                    db.complete(
                        &context.id,
                        &alice,
                        completion(),
                        "complete-one",
                        None,
                        None
                    )
                    .await
                    .unwrap()
                    .id,
                    first.id
                );
                assert!(
                    db.complete(&context.id, &bot, completion(), "complete-one", None, None)
                        .await
                        .is_err()
                );
                let resolved = db
                    .resolve(&next.id, &c.thread_id, &bot, true, None)
                    .await
                    .unwrap();
                assert_eq!(resolved.author.id, alice.id);
                assert_eq!(resolved.resolved_by.unwrap().id, bot.id);
                let reopened = db
                    .resolve(&next.id, &c.thread_id, &alice, false, None)
                    .await
                    .unwrap();
                assert!(reopened.resolved_at.is_none() && reopened.resolved_by.is_none());
                assert_eq!(reopened.updated_by.unwrap().id, alice.id);
                db.reply(
                    &next.id,
                    &c.thread_id,
                    &alice,
                    "New review, same thread",
                    None,
                )
                .await
                .unwrap();
                let live = db.get_conversation(&context.id, true).await.unwrap();
                assert_eq!(live.threads.len(), 1);
                assert_eq!(live.comments.len(), 3);
                assert_eq!(live.sessions.len(), 2);
                assert!(
                    lgtm_backend::utils::export_feedback(&live, &closed)
                        .contains("New review, same thread")
                );
                assert_eq!(
                    db.get_session(&first.id)
                        .await
                        .unwrap()
                        .completion()
                        .unwrap()
                        .by
                        .id,
                    alice.id
                );
                let other = db
                    .ensure_context(notes("/other/context"), None)
                    .await
                    .unwrap();
                let other_s = db
                    .start_session(
                        &other,
                        &alice,
                        CommitContext::default(),
                        ComparisonKind::Legacy,
                        None,
                    )
                    .await
                    .unwrap();
                assert!(
                    db.reply(&other_s.id, &c.thread_id, &alice, "wrong context", None)
                        .await
                        .is_err()
                );
                assert!(
                    db.resolve(&other_s.id, &reply.id, &alice, true, None)
                        .await
                        .is_err()
                );
                assert!(
                    db.get_selected_session(&other.id, Some(&first.id), false)
                        .await
                        .is_err()
                );
                for anchor in [
                    Anchor::File {
                        path: "../escape".into(),
                    },
                    Anchor::Line {
                        path: "src/lib.rs".into(),
                        start: 0,
                        end: 2,
                        side: "new".into(),
                        commit: "abc".into(),
                    },
                    Anchor::Line {
                        path: "a".into(),
                        start: 3,
                        end: 2,
                        side: "new".into(),
                        commit: "abc".into(),
                    },
                ] {
                    assert!(
                        db.add_comment(&next.id, &alice, "invalid", anchor, None, None)
                            .await
                            .is_err()
                    );
                }
                assert!(
                    db.add_comment(&next.id, &alice, " ", Anchor::General, None, None)
                        .await
                        .is_err()
                );
                let filtered = db
                    .get_sessions(&SessionFilter {
                        path: Some(path.clone()),
                        git_dir: None,
                        completed: false,
                    })
                    .await
                    .unwrap();
                assert_eq!(filtered.len(), 1);
                assert_eq!(filtered[0].1.id, next.id);
                assert!(
                    db.get_sessions(&SessionFilter {
                        path: Some(format!("{path}-sibling")),
                        ..Default::default()
                    })
                    .await
                    .unwrap()
                    .is_empty()
                );
                assert!(
                    db.auth()
                        .ensure_agent("Bot", "", "test-provider")
                        .await
                        .is_err()
                );
                assert_eq!(
                    db.auth().get_identity(&bot.id).await.unwrap().model(),
                    Some("model-one")
                );
                // Concurrent reply positions must be atomic on both backends.
                let mut writes = vec![];
                for i in 0..8 {
                    let db = db.clone();
                    let s = next.id;
                    let t = c.thread_id;
                    let a = alice.clone();
                    writes.push(tokio::spawn(async move {
                        db.reply(&s, &t, &a, &format!("reply {i}"), None)
                            .await
                            .unwrap()
                    }));
                }
                let mut positions = std::collections::HashSet::new();
                for handle in writes {
                    assert!(positions.insert(handle.await.unwrap().position));
                }
                // Database-side constraints remain effective even if a client bypasses services.
                let raw = sea_orm::Database::connect(&url).await.unwrap();
                // Completed review metadata is immutable through the domain service.
                assert!(
                    db.add_comment(&closed.id, &alice, "tampered", Anchor::General, None, None)
                        .await
                        .is_err()
                );
                assert!({
                    let mut row = db
                        .database()
                        .comments
                        .get_by_id(&context.id, &c.id)
                        .await
                        .unwrap()
                        .unwrap();
                    row.context_id = other.id;
                    matches!(
                        db.database().comments.update(&row).await,
                        Ok(false) | Err(_)
                    )
                });
                raw.close().await.unwrap();
                db.database().close().await;
                let reopened = common::service(&url).await.unwrap();
                assert_eq!(
                    reopened
                        .get_conversation(&context.id, true)
                        .await
                        .unwrap()
                        .comments
                        .len(),
                    11
                );
                reopened.database().close().await;
            }
        }

        #[tokio::test]
        async fn should_filter_branch_reviews_and_validate_commit_anchors() {
            for (db, url, temp) in databases().await {
                let a = actor(&db, "Reviewer", None).await;
                let root = temp.path().to_str().unwrap();
                let c = db
                    .ensure_context(
                        ContextKey::Local {
                            location: LocalLocation::Branch {
                                git_dir: "/repos/acme/repo".into(),
                                branch: "refs/heads/feature/one".into(),
                            },
                        },
                        Some(root),
                    )
                    .await
                    .unwrap();
                let s = db
                    .start_session(
                        &c,
                        &a,
                        CommitContext {
                            base: Some("old".into()),
                            head: Some("head-1".into()),
                        },
                        ComparisonKind::Legacy,
                        None,
                    )
                    .await
                    .unwrap();
                let line = db
                    .add_comment(
                        &s.id,
                        &a,
                        "Line range",
                        Anchor::Line {
                            path: "file.rs".into(),
                            start: 1,
                            end: 3,
                            side: "new".into(),
                            commit: "head-1".into(),
                        },
                        None,
                        None,
                    )
                    .await
                    .unwrap();
                let general = db
                    .add_comment(&s.id, &a, "General", Anchor::General, None, None)
                    .await
                    .unwrap();
                assert!(
                    db.resolve(&s.id, &line.id, &a, true, None).await.is_err(),
                    "A reply/comment ID is not a thread ID"
                );
                assert!(
                    db.complete(
                        &c.id,
                        &a,
                        Completion {
                            outcome: Some("approve".into()),
                            body: None,
                            submit: false
                        },
                        "invalid",
                        None,
                        None
                    )
                    .await
                    .is_err()
                );
                db.complete(&c.id, &a, completion(), "first", None, None)
                    .await
                    .unwrap();
                let next = db
                    .start_session(
                        &c,
                        &a,
                        CommitContext {
                            base: Some("head-1".into()),
                            head: Some("head-2".into()),
                        },
                        ComparisonKind::Legacy,
                        None,
                    )
                    .await
                    .unwrap();
                let live = db.get_conversation(&c.id, true).await.unwrap();
                assert!(
                    live.threads
                        .iter()
                        .find(|t| t.id == line.thread_id)
                        .unwrap()
                        .outdated
                );
                assert!(
                    !live
                        .threads
                        .iter()
                        .find(|t| t.id == general.thread_id)
                        .unwrap()
                        .outdated
                );
                for filter in [
                    SessionFilter {
                        path: None,
                        git_dir: Some("/repos/acme/repo".into()),
                        completed: false,
                    },
                    SessionFilter {
                        path: Some(format!("{root}/child")),
                        git_dir: Some("/repos/acme/repo".into()),
                        completed: false,
                    },
                ] {
                    assert_eq!(db.get_sessions(&filter).await.unwrap().len(), 1);
                }
                assert!(
                    db.get_sessions(&SessionFilter {
                        git_dir: Some("/repos/acme/other".into()),
                        ..Default::default()
                    })
                    .await
                    .unwrap()
                    .is_empty()
                );
                assert_eq!(
                    db.get_sessions(&SessionFilter {
                        completed: true,
                        ..Default::default()
                    })
                    .await
                    .unwrap()
                    .len(),
                    2
                );
                let raw = sea_orm::Database::connect(&url).await.unwrap();
                assert!({
                    let mut row = db
                        .database()
                        .comments
                        .get_thread(&c.id, &line.thread_id)
                        .await
                        .unwrap()
                        .unwrap();
                    let original_start = row.start_line;
                    row.start_line = None;
                    assert!(db.database().comments.update_thread(&row).await.unwrap());
                    db.database()
                        .comments
                        .get_thread(&c.id, &line.thread_id)
                        .await
                        .unwrap()
                        .unwrap()
                        .start_line
                        == original_start
                });
                raw.close().await.unwrap();
                let mut completions = vec![];
                for i in 0..6 {
                    let db = db.clone();
                    let a = a.clone();
                    let context = c.id;
                    completions.push(tokio::spawn(async move {
                        db.complete(
                            &context,
                            &a,
                            completion(),
                            &format!("finish-{i}"),
                            None,
                            None,
                        )
                        .await
                    }));
                }
                let mut successes = 0;
                for handle in completions {
                    if let Ok(completed) = handle.await.unwrap() {
                        successes += 1;
                        assert_eq!(completed.id, next.id);
                    }
                }
                assert_eq!(successes, 1, "Only one distinct completion may win");
                db.database().close().await;
            }
        }

        #[tokio::test]
        async fn should_resolve_updated_identity_profiles_and_validate_reviewed_commits() {
            for (db, url, _) in databases().await {
                let a = actor(&db, "Alice", None).await;
                let c = db.ensure_context(notes("/authorship"), None).await.unwrap();
                let s = db
                    .start_session(
                        &c,
                        &a,
                        CommitContext::default(),
                        ComparisonKind::Legacy,
                        None,
                    )
                    .await
                    .unwrap();
                let root = db
                    .add_comment(
                        &s.id,
                        &a,
                        "Original attribution",
                        Anchor::General,
                        None,
                        None,
                    )
                    .await
                    .unwrap();
                assert!(
                    db.add_comment(
                        &s.id,
                        &a,
                        "Missing reviewed head",
                        Anchor::Line {
                            path: "file.rs".into(),
                            start: 1,
                            end: 1,
                            side: "new".into(),
                            commit: "invented".into()
                        },
                        None,
                        None
                    )
                    .await
                    .is_err()
                );
                db.resolve(&s.id, &root.thread_id, &a, true, None)
                    .await
                    .unwrap();
                let completed = db
                    .complete(&c.id, &a, completion(), "profile-retry", None, None)
                    .await
                    .unwrap();
                let raw = sea_orm::Database::connect(&url).await.unwrap();
                use sea_orm::{ConnectionTrait, DbBackend, Statement};
                raw.execute_raw(Statement::from_sql_and_values(
                    raw.get_database_backend(),
                    match raw.get_database_backend() {
                        DbBackend::Postgres => {
                            "UPDATE identities SET name = $1, email = $2 WHERE id = $3"
                        }
                        _ => "UPDATE identities SET name = ?, email = ? WHERE id = ?",
                    },
                    [
                        "Renamed profile".into(),
                        "renamed@example.com".into(),
                        a.id.into(),
                    ],
                ))
                .await
                .unwrap();
                assert_eq!(
                    db.auth().get_identity(&a.id).await.unwrap().name,
                    "Renamed profile"
                );
                assert_eq!(
                    db.get_session(&s.id).await.unwrap().creator.name,
                    "Renamed profile"
                );
                let conversation = db.get_conversation(&c.id, true).await.unwrap();
                assert_eq!(conversation.comments[0].author.name, "Renamed profile");
                assert_eq!(conversation.threads[0].author.name, "Renamed profile");
                assert_eq!(
                    conversation.sessions[0].completion().unwrap().by.name,
                    "Renamed profile"
                );
                assert_eq!(
                    conversation.threads[0]
                        .resolved_by
                        .as_ref()
                        .unwrap()
                        .email(),
                    Some("renamed@example.com")
                );
                assert_eq!(
                    conversation.threads[0].updated_by.as_ref().unwrap().name,
                    "Renamed profile"
                );
                let current = db.auth().get_identity(&a.id).await.unwrap();
                let replay = db
                    .complete(&c.id, &current, completion(), "profile-retry", None, None)
                    .await
                    .unwrap();
                assert_eq!(replay.id, completed.id);
                assert_eq!(replay.creator.name, "Renamed profile");
                let exported = lgtm_backend::utils::export_feedback(&conversation, &replay);
                assert!(exported.contains("Renamed profile"));
                assert!(!exported.contains("Alice <alice@example.com>"));
                let mut changed_model = current.clone();
                changed_model.kind = ActorKind::Agent {
                    model: "different-model".into(),
                    provider: "test-provider".into(),
                };
                assert!(
                    db.start_session(
                        &c,
                        &changed_model,
                        CommitContext::default(),
                        ComparisonKind::Legacy,
                        None
                    )
                    .await
                    .is_err()
                );

                // A remote ID must always carry its object kind for scoped uniqueness.
                assert!({
                    let mut row = db
                        .database()
                        .comments
                        .get_by_id(&c.id, &root.id)
                        .await
                        .unwrap()
                        .unwrap();
                    row.forge_comment_id = Some("unscoped".into());
                    db.database().comments.update(&row).await.is_err()
                });
                raw.close().await.unwrap();
                db.database().close().await;
            }
        }

        #[tokio::test]
        async fn should_distinguish_human_and_agent_identity_keys() {
            for (db, _, _) in databases().await {
                let human = db
                    .auth()
                    .ensure_human("Reviewer", "reviewer@example.com")
                    .await
                    .unwrap();
                let agent = db
                    .auth()
                    .ensure_agent("Reviewer", "model", "provider")
                    .await
                    .unwrap();
                assert_ne!(human.id, agent.id);
                assert_eq!(agent.email(), None);
                assert_eq!(human.model(), None);
                assert!(db.auth().ensure_agent("Bad", "model", "").await.is_err());
                assert!(db.auth().ensure_human("Bad", "").await.is_err());
                assert_eq!(
                    db.auth()
                        .ensure_agent("Reviewer", "model", "provider")
                        .await
                        .unwrap()
                        .id,
                    agent.id
                );
                let other = db
                    .auth()
                    .ensure_agent("Reviewer", "model", "other-provider")
                    .await
                    .unwrap();
                assert_ne!(other.id, agent.id);
                db.database().close().await;
            }
        }
    }
    mod transactions {
        use crate::common;
        use lgtm_backend::{models::*, services::*};
        use lgtm_db::models::comments;

        // tests --------------------------------------------------------------

        #[tokio::test]
        async fn should_preserve_immutable_fields_in_client_owned_transactions() {
            for (service, _, _) in common::databases().await {
                let db = service.database();
                let review = ReviewService::new(db.clone(), service.auth().clone());
                let actor = review
                    .auth()
                    .ensure_human("Alice", "alice@example.com")
                    .await
                    .unwrap();
                let context = review
                    .ensure_context(
                        ContextKey::Local {
                            location: LocalLocation::Notes {
                                path: "/committed".into(),
                            },
                        },
                        None,
                    )
                    .await
                    .unwrap();
                let session = review
                    .start_session(
                        &context,
                        &actor,
                        CommitContext::default(),
                        ComparisonKind::Legacy,
                        None,
                    )
                    .await
                    .unwrap();
                let comment = review
                    .add_comment(&session.id, &actor, "Root", Anchor::General, None, None)
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
    }
    mod context_boundaries {
        use crate::common;
        use lgtm_backend::{
            models::*,
            utils::{ForgeSnapshot, PostReceipt},
        };

        // tests ---------------------------------------------------------------
        #[tokio::test]
        async fn should_reject_forge_operations_for_all_local_contexts() {
            for (review, _, _) in common::databases().await {
                let actor = review
                    .auth()
                    .ensure_human("Local reviewer", "local@example.com")
                    .await
                    .unwrap();
                for location in [
                    LocalLocation::Notes {
                        path: "/notes/local-only".into(),
                    },
                    LocalLocation::Branch {
                        git_dir: "/repos/local-only/.git".into(),
                        branch: "refs/heads/main".into(),
                    },
                ] {
                    let context = review
                        .ensure_context(ContextKey::Local { location }, None)
                        .await
                        .unwrap();
                    let session = review
                        .start_session(
                            &context,
                            &actor,
                            CommitContext::default(),
                            ComparisonKind::Legacy,
                            None,
                        )
                        .await
                        .unwrap();
                    let comment = review
                        .add_comment(
                            &session.id,
                            &actor,
                            "Keep local",
                            Anchor::General,
                            None,
                            None,
                        )
                        .await
                        .unwrap();
                    let observation = ForgeSnapshot {
                        state: None,
                        complete: true,
                        head: "head".into(),
                        base: None,
                        threads: vec![],
                    };
                    assert!(
                        review
                            .apply_remote(&context, observation, true)
                            .await
                            .is_err()
                    );
                    assert!(
                        review
                            .record_refresh_failure(&context, "remote failure")
                            .await
                            .is_err()
                    );
                    assert!(
                        review
                            .stage_post(&session.id, &comment.id, "post", &comment, "Keep local")
                            .await
                            .is_err()
                    );
                    assert!(
                        review
                            .posting_uncertain(&context.id, &comment.id, "uncertain")
                            .await
                            .is_err()
                    );
                    assert!(
                        review
                            .confirm_post(
                                &context.id,
                                &comment.id,
                                "post",
                                PostReceipt {
                                    comment_id: "remote".into(),
                                    kind: "review_comment".into(),
                                    thread_id: None,
                                    review_id: None,
                                    posting_account_id: "account".into()
                                }
                            )
                            .await
                            .is_err()
                    );
                    let stored = review.get_context(&context.key).await.unwrap().unwrap();
                    assert_eq!(stored.refresh_status, "never");
                    let stored = review.get_comment(&context.id, &comment.id).await.unwrap();
                    assert!(matches!(stored.posting, PostingStatus::Unposted));
                    let mut forged = context.clone();
                    forged.key = ContextKey::ForgeBacked {
                        repository_url: ForgeRepositoryUrl::parse("https://github.com/acme/repo")
                            .unwrap(),
                        number: 1,
                    };
                    assert!(
                        review
                            .apply_remote(
                                &forged,
                                ForgeSnapshot {
                                    state: None,
                                    complete: true,
                                    head: "head".into(),
                                    base: None,
                                    threads: vec![]
                                },
                                true
                            )
                            .await
                            .is_err()
                    );
                }
                review.database().close().await;
            }
        }
    }
}

mod mutation_guards {
    use crate::common;
    use lgtm_backend::{models::*, services::ReviewService};

    // fixtures ----------------------------------------------------------------
    async fn notes(service: &ReviewService, path: &std::path::Path) -> (Context, Actor, Session) {
        let context = service
            .ensure_context(
                ContextKey::Local {
                    location: LocalLocation::Notes {
                        path: path.to_str().unwrap().into(),
                    },
                },
                None,
            )
            .await
            .unwrap();
        let actor = service
            .auth()
            .ensure_human("Reviewer", "guard@example.test")
            .await
            .unwrap();
        let session = service
            .start_session(
                &context,
                &actor,
                CommitContext::default(),
                ComparisonKind::Directory,
                None,
            )
            .await
            .unwrap();
        (context, actor, session)
    }

    async fn version(service: &ReviewService, context: &Context) -> i64 {
        service
            .get_context_by_id(&context.id)
            .await
            .unwrap()
            .revision
    }

    // tests -------------------------------------------------------------------
    #[tokio::test]
    async fn should_return_the_saved_creation_after_other_writes_without_changing_the_version() {
        for (service, _, temp) in common::databases().await {
            let (context, actor, session) = notes(&service, temp.path()).await;
            let guard = MutationGuard::new(
                Some(version(&service, &context).await),
                "create-comment".into(),
            );
            let comment = service
                .add_comment(
                    &session.id,
                    &actor,
                    "Check the timeout",
                    Anchor::General,
                    None,
                    Some(&guard),
                )
                .await
                .unwrap();
            let reply_guard =
                MutationGuard::new(Some(version(&service, &context).await), "reply".into());
            service
                .reply(
                    &session.id,
                    &comment.thread_id,
                    &actor,
                    "Keep the fallback",
                    Some(&reply_guard),
                )
                .await
                .unwrap();
            let before_retry = version(&service, &context).await;
            let retry = service
                .add_comment(
                    &session.id,
                    &actor,
                    "Check the timeout",
                    Anchor::General,
                    None,
                    Some(&guard),
                )
                .await
                .unwrap();
            assert_eq!(retry.id, comment.id);
            assert_eq!(
                version(&service, &context).await,
                before_retry,
                "retry is a saved result, not a new mutation"
            );
            assert_eq!(
                service
                    .get_conversation(&context.id, true)
                    .await
                    .unwrap()
                    .comments
                    .len(),
                2
            );
            assert!(
                service
                    .add_comment(
                        &session.id,
                        &actor,
                        "Different body",
                        Anchor::General,
                        None,
                        Some(&guard)
                    )
                    .await
                    .is_err()
            );
            assert!(
                service
                    .reply(
                        &session.id,
                        &comment.thread_id,
                        &actor,
                        "Different operation",
                        Some(&guard)
                    )
                    .await
                    .is_err()
            );
            assert_eq!(
                version(&service, &context).await,
                before_retry,
                "conflicting reuse rolls back"
            );
        }
    }

    #[tokio::test]
    async fn should_reject_a_stale_edit_without_overwriting_an_accepted_edit() {
        for (service, _, temp) in common::databases().await {
            let (context, actor, session) = notes(&service, temp.path()).await;
            let comment = service
                .add_comment(&session.id, &actor, "Original", Anchor::General, None, None)
                .await
                .unwrap();
            let seen = version(&service, &context).await;
            let accepted = MutationGuard::new(Some(seen), "first-editor".into());
            service
                .edit_comment(
                    &session.id,
                    &comment.id,
                    &actor,
                    "Accepted edit",
                    Some(&accepted),
                )
                .await
                .unwrap();
            let stale = MutationGuard::new(Some(seen), "second-editor".into());
            assert!(
                service
                    .edit_comment(&session.id, &comment.id, &actor, "Stale edit", Some(&stale))
                    .await
                    .is_err()
            );
            assert_eq!(
                service
                    .get_comment(&context.id, &comment.id)
                    .await
                    .unwrap()
                    .body,
                "Accepted edit"
            );
            let retry = service
                .edit_comment(
                    &session.id,
                    &comment.id,
                    &actor,
                    "Accepted edit",
                    Some(&accepted),
                )
                .await
                .unwrap();
            assert_eq!(retry.body, "Accepted edit");
        }
    }

    #[tokio::test]
    async fn should_retry_a_saved_source_comment_after_the_file_changes_and_backend_restarts() {
        for (service, url, temp) in common::databases().await {
            let file = temp.path().join("notes.md");
            std::fs::write(&file, b"before\r\nselected\r\nafter").unwrap();
            let (context, actor, session) = notes(&service, temp.path()).await;
            let bytes = std::fs::read(&file).unwrap();
            let evidence = AnchorEvidence::capture(
                AnchorSource::LocalFile {
                    content_hash: AnchorSource::hash(&bytes),
                },
                &bytes,
                2,
                2,
            )
            .unwrap();
            let anchor = Anchor::Line {
                path: "notes.md".into(),
                side: "new".into(),
                start: 2,
                end: 2,
                commit: evidence.source.content_hash().into(),
            };
            let guard = MutationGuard::new(
                Some(version(&service, &context).await),
                "source-comment".into(),
            );
            let saved = service
                .add_comment(
                    &session.id,
                    &actor,
                    "Explain this",
                    anchor.clone(),
                    Some(evidence.clone()),
                    Some(&guard),
                )
                .await
                .unwrap();
            std::fs::write(&file, "changed after the response was lost\n").unwrap();
            service.database().close().await;
            let restarted = common::service(&url).await.unwrap();
            let retry = restarted
                .add_comment(
                    &session.id,
                    &actor,
                    "Explain this",
                    anchor,
                    Some(evidence.clone()),
                    Some(&guard),
                )
                .await
                .unwrap();
            assert_eq!(retry.id, saved.id);
            let stored = restarted
                .database()
                .comments
                .get_thread(&context.id, &saved.thread_id)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(stored.selected_text.as_deref(), Some("selected"));
            assert_eq!(
                stored.source_content_hash.as_deref(),
                Some(evidence.source.content_hash())
            );
            assert_eq!(stored.start_line, Some(2));
        }
    }

    #[tokio::test]
    async fn should_keep_comparison_selection_and_complete_only_the_requested_attempt() {
        for (service, _, temp) in common::databases().await {
            let context = service
                .ensure_context(
                    ContextKey::Local {
                        location: LocalLocation::Branch {
                            git_dir: temp.path().join(".git").to_str().unwrap().into(),
                            branch: "refs/heads/topic".into(),
                        },
                    },
                    None,
                )
                .await
                .unwrap();
            let actor = service
                .auth()
                .ensure_human("Reviewer", "compare@example.test")
                .await
                .unwrap();
            let commits = CommitContext {
                base: Some("a".repeat(40)),
                head: Some("a".repeat(40)),
            };
            let session = service
                .start_session(
                    &context,
                    &actor,
                    commits.clone(),
                    ComparisonKind::Staged,
                    None,
                )
                .await
                .unwrap();
            assert!(
                service
                    .start_session(
                        &context,
                        &actor,
                        commits.clone(),
                        ComparisonKind::Working,
                        None
                    )
                    .await
                    .is_err()
            );
            let completed = service
                .complete(
                    &context.id,
                    &actor,
                    Completion::default(),
                    "finish-first",
                    Some(&session.id),
                    None,
                )
                .await
                .unwrap();
            assert_eq!(completed.id, session.id);
            let next = service
                .start_session(&context, &actor, commits, ComparisonKind::Branch, None)
                .await
                .unwrap();
            assert!(
                service
                    .complete(
                        &context.id,
                        &actor,
                        Completion::default(),
                        "wrong-attempt",
                        Some(&session.id),
                        None
                    )
                    .await
                    .is_err()
            );
            assert_eq!(
                service.get_session(&next.id).await.unwrap().state(),
                "active"
            );
        }
    }
}
