use lgtm_backend::models::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn parallel_first_install_and_foreign_keys_on_each_connection() {
    let temp = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}?mode=rwc",
        temp.path().join("parallel.sqlite").display()
    );
    let mut tasks = vec![];
    for _ in 0..8 {
        let url = url.clone();
        tasks.push(tokio::spawn(async move {
            support::service(&url).await.unwrap()
        }));
    }
    for task in tasks {
        task.await.unwrap().database().close().await;
    }
    let db = support::service(&url).await.unwrap();
    let actor = db
        .auth()
        .ensure_identity("A", "a@example.com", None, None)
        .await
        .unwrap();
    let context = db
        .ensure_context(
            ContextKey::Notes {
                path: "/notes".into(),
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
        actor.id = "missing identity".into();
        tasks.push(tokio::spawn(async move {
            db.start_session(&c, &actor, CommitContext::default()).await
        }));
    }
    for task in tasks {
        assert!(task.await.unwrap().is_err());
    }
    assert!(db.selected_session(&context.id, None, true).await.is_err());
    db.database().close().await;
}

mod support;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn independent_pools_share_context_keys_and_serialize_start_complete_races() {
    for (db, url, _) in support::databases().await {
        let actor = db
            .auth()
            .ensure_identity("Alice", "alice@example.com", None, None)
            .await
            .unwrap();
        for key in [
            ContextKey::Notes {
                path: "/race-notes".into(),
            },
            ContextKey::Branch {
                repository: "github.com/acme/race".into(),
                branch: "refs/heads/topic".into(),
            },
            ContextKey::PullRequest {
                host: "github.com".into(),
                repository: "github.com/acme/race".into(),
                number: 42,
            },
        ] {
            let mut tasks = vec![];
            for _ in 0..4 {
                let url = url.clone();
                let key = key.clone();
                tasks.push(tokio::spawn(async move {
                    let other = support::service(&url).await.unwrap();
                    let context = other.ensure_context(key, None).await.unwrap();
                    other.database().close().await;
                    context
                }));
            }
            let context = tasks.remove(0).await.unwrap();
            for task in tasks {
                assert_eq!(task.await.unwrap().id, context.id);
            }
            let context = if context.key.kind() == "pr" {
                db.apply_remote(
                    &context,
                    ForgeSnapshot {
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
                    let other = support::service(&url).await.unwrap();
                    let session = other
                        .start_session(&context, &actor, CommitContext::default())
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
            if context.key.kind() != "pr" {
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
                        )
                        .await
                        .unwrap()
                });
                let a = actor.clone();
                let c = context.clone();
                let other = db.clone();
                let start = tokio::spawn(async move {
                    other
                        .start_session(&c, &a, CommitContext::default())
                        .await
                        .unwrap()
                });
                assert_eq!(complete.await.unwrap().id, session.id);
                let raced = start.await.unwrap();
                let successor = db
                    .start_session(&context, &actor, CommitContext::default())
                    .await
                    .unwrap();
                assert_ne!(successor.id, session.id);
                assert!(raced.id == session.id || raced.id == successor.id);
            }
        }
        db.database().close().await;
    }
}
