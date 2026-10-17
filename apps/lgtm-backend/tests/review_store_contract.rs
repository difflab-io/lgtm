use lgtm_backend::{models::*, services::*};

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

fn completion() -> Completion {
    Completion {
        outcome: None,
        body: Some("Ready".into()),
        submit: false,
    }
}

fn notes(path: &str) -> ContextKey {
    ContextKey::Notes { path: path.into() }
}

#[tokio::test]
async fn shared_lifecycle_integrity_concurrency_and_current_exports() {
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
                db.start_session(&c, &a, CommitContext::default())
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
            )
            .await
            .unwrap();
        let reply = db
            .reply(&first.id, &c.thread_id, &bot, "Agent reply")
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
            .complete(&context.id, &alice, completion(), "complete-one")
            .await
            .unwrap();
        assert_eq!(
            closed.completion().map(|c| c.outcome.as_str()),
            Some("saved")
        );
        assert!(
            db.add_comment(&first.id, &alice, "closed", Anchor::General)
                .await
                .is_err()
        );
        let next = db
            .start_session(&context, &bot, CommitContext::default())
            .await
            .unwrap();
        assert_ne!(next.id, first.id);
        assert_eq!(
            db.complete(&context.id, &alice, completion(), "complete-one")
                .await
                .unwrap()
                .id,
            first.id
        );
        assert!(
            db.complete(&context.id, &bot, completion(), "complete-one")
                .await
                .is_err()
        );
        let resolved = db
            .resolve(&next.id, &c.thread_id, &bot, true)
            .await
            .unwrap();
        assert_eq!(resolved.author.id, alice.id);
        assert_eq!(resolved.resolved_by.unwrap().id, bot.id);
        let reopened = db
            .resolve(&next.id, &c.thread_id, &alice, false)
            .await
            .unwrap();
        assert!(reopened.resolved_at.is_none() && reopened.resolved_by.is_none());
        assert_eq!(reopened.updated_by.unwrap().id, alice.id);
        db.reply(&next.id, &c.thread_id, &alice, "New review, same thread")
            .await
            .unwrap();
        let live = db.get_conversation(&context.id, true).await.unwrap();
        assert_eq!(live.threads.len(), 1);
        assert_eq!(live.comments.len(), 3);
        assert_eq!(live.sessions.len(), 2);
        assert!(ReviewService::export(&live, &closed).contains("New review, same thread"));
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
            .start_session(&other, &alice, CommitContext::default())
            .await
            .unwrap();
        assert!(
            db.reply(&other_s.id, &c.thread_id, &alice, "wrong context")
                .await
                .is_err()
        );
        assert!(
            db.resolve(&other_s.id, &reply.id, &alice, true)
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
                db.add_comment(&next.id, &alice, "invalid", anchor)
                    .await
                    .is_err()
            );
        }
        assert!(
            db.add_comment(&next.id, &alice, " ", Anchor::General)
                .await
                .is_err()
        );
        let filtered = db
            .get_sessions(&SessionFilter {
                path: Some(path.clone()),
                repository: None,
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
                db.reply(&s, &t, &a, &format!("reply {i}")).await.unwrap()
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
            db.add_comment(&closed.id, &alice, "tampered", Anchor::General)
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
        let reopened = support::service(&url).await.unwrap();
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
async fn shared_branch_filters_commit_context_and_anchor_invariants() {
    for (db, url, temp) in databases().await {
        let a = actor(&db, "Reviewer", None).await;
        let root = temp.path().to_str().unwrap();
        let c = db
            .ensure_context(
                ContextKey::Branch {
                    repository: "github.com/acme/repo".into(),
                    branch: "refs/heads/feature/one".into(),
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
            )
            .await
            .unwrap();
        let general = db
            .add_comment(&s.id, &a, "General", Anchor::General)
            .await
            .unwrap();
        assert!(
            db.resolve(&s.id, &line.id, &a, true).await.is_err(),
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
                "invalid"
            )
            .await
            .is_err()
        );
        db.complete(&c.id, &a, completion(), "first").await.unwrap();
        let next = db
            .start_session(
                &c,
                &a,
                CommitContext {
                    base: Some("head-1".into()),
                    head: Some("head-2".into()),
                },
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
                repository: Some("github.com/acme/repo".into()),
                completed: false,
            },
            SessionFilter {
                path: Some(format!("{root}/child")),
                repository: Some("github.com/acme/repo".into()),
                completed: false,
            },
        ] {
            assert_eq!(db.get_sessions(&filter).await.unwrap().len(), 1);
        }
        assert!(
            db.get_sessions(&SessionFilter {
                repository: Some("github.com/acme/other".into()),
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
                db.complete(&context, &a, completion(), &format!("finish-{i}"))
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
async fn identity_relations_follow_profile_changes_and_validate_reviewed_commits() {
    for (db, url, _) in databases().await {
        let a = actor(&db, "Alice", None).await;
        let c = db.ensure_context(notes("/authorship"), None).await.unwrap();
        let s = db
            .start_session(&c, &a, CommitContext::default())
            .await
            .unwrap();
        let root = db
            .add_comment(&s.id, &a, "Original attribution", Anchor::General)
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
                }
            )
            .await
            .is_err()
        );
        db.resolve(&s.id, &root.thread_id, &a, true).await.unwrap();
        let completed = db
            .complete(&c.id, &a, completion(), "profile-retry")
            .await
            .unwrap();
        let raw = sea_orm::Database::connect(&url).await.unwrap();
        use sea_orm::{ConnectionTrait, DbBackend, Statement};
        raw.execute_raw(Statement::from_sql_and_values(
            raw.get_database_backend(),
            match raw.get_database_backend() {
                DbBackend::Postgres => "UPDATE identities SET name = $1, email = $2 WHERE id = $3",
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
            .complete(&c.id, &current, completion(), "profile-retry")
            .await
            .unwrap();
        assert_eq!(replay.id, completed.id);
        assert_eq!(replay.creator.name, "Renamed profile");
        let exported = ReviewService::export(&conversation, &replay);
        assert!(exported.contains("Renamed profile <renamed@example.com>"));
        assert!(!exported.contains("Alice <alice@example.com>"));
        let mut changed_model = current.clone();
        changed_model.kind = ActorKind::Agent {
            model: "different-model".into(),
            provider: "test-provider".into(),
        };
        assert!(
            db.start_session(&c, &changed_model, CommitContext::default())
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
async fn human_and_agent_identity_shapes_and_keys_are_distinct() {
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
