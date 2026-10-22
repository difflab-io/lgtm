//! Stable saved-file and directory review locations without rewriting original anchors.
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
