//! Compatibility for saved notes without rewriting legacy conversations or their original anchors.
mod common;
use lgtm_backend::models::*;

// fixtures --------------------------------------------------------------------
fn evidence(bytes: &[u8]) -> AnchorEvidence {
    AnchorEvidence::capture(
        AnchorSource::LocalFile {
            content_hash: AnchorSource::hash(bytes),
        },
        bytes,
        2,
        2,
    )
    .unwrap()
}

// tests -----------------------------------------------------------------------
#[tokio::test]
async fn should_resume_legacy_directory_notes_by_file_without_exposing_neighboring_feedback() {
    for (service, _, temporary) in common::databases().await {
        let root = temporary.path().join("legacy-notes");
        std::fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let bytes = b"before\nselected\nafter\n";
        for name in ["one.md", "two.md"] {
            std::fs::write(root.join(name), bytes).unwrap();
        }
        let actor = service
            .auth()
            .ensure_human("Reviewer", "reviewer@example.test")
            .await
            .unwrap();
        let context = service
            .ensure_context(
                ContextKey::Local {
                    location: LocalLocation::Notes {
                        path: root.to_string_lossy().into_owned(),
                    },
                },
                None,
            )
            .await
            .unwrap();
        let legacy = service
            .start_session(
                &context,
                &actor,
                CommitContext::default(),
                ComparisonKind::Legacy,
                None,
            )
            .await
            .unwrap();
        let mut originals = vec![];
        for name in ["one.md", "two.md"] {
            originals.push(
                service
                    .add_comment(
                        &legacy.id,
                        &actor,
                        name,
                        Anchor::Line {
                            path: name.into(),
                            start: 2,
                            end: 2,
                            side: "new".into(),
                            commit: AnchorSource::hash(bytes),
                        },
                        Some(evidence(bytes)),
                        None,
                    )
                    .await
                    .unwrap(),
            );
        }
        service
            .complete(
                &context.id,
                &actor,
                Completion::default(),
                "finish-legacy",
                Some(&legacy.id),
                None,
            )
            .await
            .unwrap();
        let scope = ReviewScope::Notes {
            path: root.join("one.md").to_string_lossy().into_owned(),
        };
        let first = service
            .get_scope_conversation(&scope)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(first.context.id, context.id);
        assert_eq!(first.comments.len(), 1);
        assert_eq!(first.comments[0].id, originals[0].id);
        assert!(matches!(&first.threads[0].anchor, Anchor::Line { path, .. } if path == "one.md"));
        let writer = service.resolve_write_scope(&scope, &actor).await.unwrap();
        assert_eq!(writer.comparison_kind, ComparisonKind::Notes);
        service
            .add_comment(
                &writer.id,
                &actor,
                "New note keeps the old file conversation",
                Anchor::Line {
                    path: "one.md".into(),
                    start: 2,
                    end: 2,
                    side: "new".into(),
                    commit: AnchorSource::hash(bytes),
                },
                Some(evidence(bytes)),
                None,
            )
            .await
            .unwrap();
        assert!(
            service
                .get_reviews(None, Some("active"), None, 100)
                .await
                .unwrap()
                .is_empty(),
            "the notes writer is not a visible review"
        );
        std::fs::remove_dir_all(&root).unwrap();
        let retained = service
            .get_scope_conversation(&scope)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            retained.comments.len(),
            2,
            "deleting the directory cannot reveal its neighboring file discussion"
        );
        assert!(
            retained
                .comments
                .iter()
                .all(|comment| comment.body != "two.md")
        );
        assert!(
            retained
                .comments
                .iter()
                .any(|comment| comment.id == originals[0].id)
        );
        let exported = service.get_export(&scope, &[]).await.unwrap();
        assert!(exported.contains("New note keeps the old file conversation"));
        assert!(!exported.contains("two.md"));
        service.database().close().await;
    }
}

#[tokio::test]
async fn should_keep_file_notes_and_new_origins_across_repeated_moves_between_parents() {
    for (service, _, temporary) in common::databases().await {
        let root = temporary.path().canonicalize().unwrap();
        for directory in ["before", "middle", "after"] {
            std::fs::create_dir(root.join(directory)).unwrap();
        }
        let bytes = b"before\nselected\nafter\n";
        let first = root.join("before/one.md");
        let middle = root.join("middle/two.md");
        let last = root.join("after/three.md");
        std::fs::write(&first, bytes).unwrap();
        let actor = service
            .auth()
            .ensure_human("Reviewer", "moves@example.test")
            .await
            .unwrap();
        let scope = |path: &std::path::Path| ReviewScope::Notes {
            path: path.to_string_lossy().into_owned(),
        };
        let anchor = |name: &str| Anchor::Line {
            path: name.into(),
            start: 2,
            end: 2,
            side: "new".into(),
            commit: AnchorSource::hash(bytes),
        };
        let first_scope = scope(&first);
        let writer = service
            .resolve_write_scope(&first_scope, &actor)
            .await
            .unwrap();
        let original = service
            .add_comment(
                &writer.id,
                &actor,
                "Keep this note",
                anchor("one.md"),
                Some(evidence(bytes)),
                Some(
                    &MutationGuard::new(Some(0), "first-note".into()).in_scope(first_scope.clone()),
                ),
            )
            .await
            .unwrap();
        let first_read = service
            .get_scope_conversation(&first_scope)
            .await
            .unwrap()
            .unwrap();
        std::fs::rename(&first, &middle).unwrap();
        let change = SourceChange::LocationMoved {
            previous_path: first.to_string_lossy().into_owned(),
            current_path: middle.to_string_lossy().into_owned(),
        };
        let guard = MutationGuard::new(Some(first_read.context.revision), "move-first".into());
        let version = service
            .refresh_locations(&first_scope, &actor, std::slice::from_ref(&change), &guard)
            .await
            .unwrap();
        assert_eq!(
            service
                .refresh_locations(&first_scope, &actor, &[change], &guard)
                .await
                .unwrap(),
            version,
            "same move retry keeps its receipt"
        );
        let middle_scope = scope(&middle);
        let moved = service
            .get_scope_conversation(&middle_scope)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(moved.comments[0].id, original.id);
        assert_eq!(moved.threads[0].source_path.as_deref(), first.to_str());
        assert_eq!(moved.threads[0].current_path.as_deref(), middle.to_str());
        let second = service
            .add_comment(
                &writer.id,
                &actor,
                "New note after the move",
                anchor("two.md"),
                Some(evidence(bytes)),
                Some(
                    &MutationGuard::new(Some(moved.context.revision), "second-note".into())
                        .in_scope(middle_scope.clone()),
                ),
            )
            .await
            .unwrap();
        let updated = service
            .get_scope_conversation(&middle_scope)
            .await
            .unwrap()
            .unwrap();
        std::fs::rename(&middle, &last).unwrap();
        service
            .refresh_locations(
                &middle_scope,
                &actor,
                &[SourceChange::LocationMoved {
                    previous_path: middle.to_string_lossy().into_owned(),
                    current_path: last.to_string_lossy().into_owned(),
                }],
                &MutationGuard::new(Some(updated.context.revision), "move-second".into()),
            )
            .await
            .unwrap();
        let final_read = service
            .get_scope_conversation(&scope(&last))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(final_read.context.id, writer.context_id);
        assert_eq!(final_read.comments.len(), 2);
        for thread in &final_read.threads {
            assert_eq!(thread.current_path.as_deref(), last.to_str());
            let expected = if thread.id == original.thread_id {
                &first
            } else {
                assert_eq!(thread.id, second.thread_id);
                &middle
            };
            assert_eq!(thread.source_path.as_deref(), expected.to_str());
            assert!(thread.orphan_reason.is_none());
        }
        std::fs::write(&first, b"unrelated replacement\n").unwrap();
        assert!(
            service
                .get_scope_conversation(&first_scope)
                .await
                .unwrap()
                .unwrap()
                .comments
                .is_empty(),
            "reusing the original name cannot move notes back"
        );
        assert!(
            service
                .get_reviews(None, Some("active"), None, 100)
                .await
                .unwrap()
                .is_empty()
        );
        service.database().close().await;
    }
}

#[tokio::test]
async fn should_resume_directory_reviews_after_repeated_root_moves_without_rewriting_origins() {
    for (service, _, temporary) in common::databases().await {
        let root = temporary.path().canonicalize().unwrap();
        let first = root.join("ignored-plan");
        let middle = root.join("renamed-plan");
        let last = root.join("final-plan");
        std::fs::create_dir(&first).unwrap();
        let bytes = b"before\nselected\nafter\n";
        std::fs::write(first.join("PLAN.md"), bytes).unwrap();
        let actor = service
            .auth()
            .ensure_human("Reviewer", "directory@example.test")
            .await
            .unwrap();
        let target = |path: &std::path::Path| ReviewTarget::Directory {
            path: path.to_string_lossy().into_owned(),
        };
        let context = service.resolve_target(&target(&first)).await.unwrap();
        let review = service
            .start_session(
                &context,
                &actor,
                CommitContext::default(),
                ComparisonKind::Directory,
                None,
            )
            .await
            .unwrap();
        let scope = ReviewScope::Review {
            review_id: review.id,
        };
        let version = service
            .get_scope_conversation(&scope)
            .await
            .unwrap()
            .unwrap()
            .context
            .revision;
        let anchor = Anchor::Line {
            path: "PLAN.md".into(),
            start: 2,
            end: 2,
            side: "new".into(),
            commit: AnchorSource::hash(bytes),
        };
        let comment = service
            .add_comment(
                &review.id,
                &actor,
                "Directory feedback",
                anchor.clone(),
                Some(evidence(bytes)),
                Some(
                    &MutationGuard::new(Some(version), "directory-comment".into())
                        .in_scope(scope.clone()),
                ),
            )
            .await
            .unwrap();
        for (index, (from, to)) in [(&first, &middle), (&middle, &last)]
            .into_iter()
            .enumerate()
        {
            let before = service
                .get_scope_conversation(&scope)
                .await
                .unwrap()
                .unwrap();
            std::fs::rename(from, to).unwrap();
            service
                .refresh_locations(
                    &scope,
                    &actor,
                    &[SourceChange::LocationMoved {
                        previous_path: from.to_string_lossy().into_owned(),
                        current_path: to.to_string_lossy().into_owned(),
                    }],
                    &MutationGuard::new(
                        Some(before.context.revision),
                        format!("root-move-{index}"),
                    ),
                )
                .await
                .unwrap();
            let selected = service.get_review(&review.id).await.unwrap();
            assert_eq!(selected.target, target(to));
            assert_eq!(
                service.resolve_target(&target(to)).await.unwrap().id,
                context.id
            );
            let read = service
                .get_scope_conversation(&scope)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(read.comments[0].id, comment.id);
            assert_eq!(read.threads[0].anchor, anchor);
            assert_eq!(
                read.threads[0].source_path,
                Some(first.join("PLAN.md").to_string_lossy().into_owned())
            );
            assert_eq!(
                read.threads[0].current_path,
                Some(to.join("PLAN.md").to_string_lossy().into_owned())
            );
        }
        assert_eq!(
            service
                .get_reviews(Some(&target(&last)), None, None, 100)
                .await
                .unwrap()
                .len(),
            1
        );
        let file_scope = ReviewScope::Notes {
            path: last.join("PLAN.md").to_string_lossy().into_owned(),
        };
        assert_eq!(
            service
                .get_scope_conversation(&file_scope)
                .await
                .unwrap()
                .unwrap()
                .comments[0]
                .id,
            comment.id
        );
        service.database().close().await;
    }
}
