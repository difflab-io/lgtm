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

#[tokio::test]
async fn should_observe_saved_source_deletion_and_restoration_atomically_with_stable_retries() {
    for (service, _, temporary) in common::databases().await {
        // Arrange
        let path = temporary.path().canonicalize().unwrap().join("observed.md");
        let bytes = b"before\nselected\nafter\n";
        std::fs::write(&path, bytes).unwrap();
        let scope = ReviewScope::Notes {
            path: path.to_string_lossy().into_owned(),
        };
        let actor = service
            .auth()
            .ensure_human("Observer", "observer@example.test")
            .await
            .unwrap();
        let writer = service.resolve_write_scope(&scope, &actor).await.unwrap();
        service
            .add_comment(
                &writer.id,
                &actor,
                "Keep this discussion",
                Anchor::Line {
                    path: "observed.md".into(),
                    start: 2,
                    end: 2,
                    side: "new".into(),
                },
                Some(evidence(bytes)),
                Some(&MutationGuard::new(Some(0), "observed-note".into()).in_scope(scope.clone())),
            )
            .await
            .unwrap();
        let original = service
            .get_scope_conversation(&scope)
            .await
            .unwrap()
            .unwrap();
        let version = original.context.revision;

        // Act / Assert: observing unchanged bytes is a read, and failed reads never imply deletion.
        assert_eq!(
            service
                .observe_sources(
                    &scope,
                    &actor,
                    "observed.md",
                    &MutationGuard::new(Some(version), "unchanged".into())
                )
                .await
                .unwrap(),
            version
        );
        std::fs::write(&path, [0, 255]).unwrap();
        assert!(
            service
                .observe_sources(
                    &scope,
                    &actor,
                    "observed.md",
                    &MutationGuard::new(Some(version), "unreadable".into())
                )
                .await
                .is_err()
        );
        let unchanged = service
            .get_scope_conversation(&scope)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(unchanged.context.revision, version);
        assert_eq!(unchanged.threads[0].orphan_reason, None);

        // Act / Assert: the backend identifies a deletion without client-supplied changes.
        std::fs::remove_file(&path).unwrap();
        let guard = MutationGuard::new(Some(version), "observe-deletion".into());
        let deleted_version = service
            .observe_sources(&scope, &actor, "observed.md", &guard)
            .await
            .unwrap();
        let deleted = service
            .get_scope_conversation(&scope)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            deleted.threads[0].orphan_reason,
            Some(OrphanReason::FileDeleted)
        );
        assert_eq!(deleted_version, version + 1);

        // A retry returns the accepted receipt even if source bytes changed since the first request.
        std::fs::write(&path, b"intro\nbefore\nselected\nafter\n").unwrap();
        assert_eq!(
            service
                .observe_sources(&scope, &actor, "observed.md", &guard)
                .await
                .unwrap(),
            deleted_version
        );
        let still_deleted = service
            .get_scope_conversation(&scope)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            still_deleted.threads[0].orphan_reason,
            Some(OrphanReason::FileDeleted)
        );
        service
            .observe_sources(
                &scope,
                &actor,
                "observed.md",
                &MutationGuard::new(Some(deleted_version), "observe-restoration".into()),
            )
            .await
            .unwrap();
        let restored = service
            .get_scope_conversation(&scope)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(restored.threads[0].orphan_reason, None);
        assert_eq!(restored.threads[0].anchor, original.threads[0].anchor);
        assert_eq!(restored.threads[0].evidence, original.threads[0].evidence);
        assert_eq!(restored.comments[0].id, original.comments[0].id);
        assert!(
            service
                .observe_sources(
                    &scope,
                    &actor,
                    "other.md",
                    &MutationGuard::new(Some(restored.context.revision), "wrong-file".into())
                )
                .await
                .is_err()
        );
        service.database().close().await;
    }
}

#[tokio::test]
async fn should_reconcile_moves_on_backend_reads_without_client_path_observations() {
    for (service, url, temporary) in common::databases().await {
        let root = temporary.path().canonicalize().unwrap();
        let original = root.join("original.md");
        let destination = root.join("renamed.md");
        let copy = root.join("copy.md");
        let bytes = b"before\nselected\nafter\n";
        std::fs::write(&original, bytes).unwrap();
        let actor = service
            .auth()
            .ensure_human("Reviewer", "backend-moves@example.test")
            .await
            .unwrap();
        let scope = |path: &std::path::Path| ReviewScope::Notes {
            path: path.to_string_lossy().into_owned(),
        };
        let writer = service
            .resolve_write_scope(&scope(&original), &actor)
            .await
            .unwrap();
        let comment = service
            .add_comment(
                &writer.id,
                &actor,
                "Preserve this note",
                Anchor::Line {
                    path: "original.md".into(),
                    start: 2,
                    end: 2,
                    side: "new".into(),
                },
                Some(evidence(bytes)),
                Some(
                    &MutationGuard::new(Some(0), "backend-note".into()).in_scope(scope(&original)),
                ),
            )
            .await
            .unwrap();

        // A copy has identical bytes but is a separate filesystem object.
        std::fs::copy(&original, &copy).unwrap();
        assert!(
            service
                .get_scope_conversation(&scope(&copy))
                .await
                .unwrap()
                .is_none()
        );
        let before = service
            .get_scope_conversation(&scope(&original))
            .await
            .unwrap()
            .unwrap();
        std::fs::rename(&original, &destination).unwrap();
        let moved = service
            .get_scope_conversation(&scope(&destination))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(moved.comments[0].id, comment.id);
        assert_eq!(moved.threads[0].source_path.as_deref(), original.to_str());
        assert_eq!(
            moved.threads[0].current_path.as_deref(),
            destination.to_str()
        );
        assert_eq!(moved.context.revision, before.context.revision + 1);
        assert_eq!(
            service
                .get_scope_conversation(&scope(&destination))
                .await
                .unwrap()
                .unwrap()
                .context
                .revision,
            moved.context.revision
        );
        let restarted = common::service(&url).await.unwrap();
        assert_eq!(
            restarted
                .get_scope_conversation(&scope(&destination))
                .await
                .unwrap()
                .unwrap()
                .comments[0]
                .id,
            comment.id
        );

        let directory = root.join("directory");
        let renamed_directory = root.join("renamed-directory");
        std::fs::create_dir(&directory).unwrap();
        std::fs::write(directory.join("PLAN.md"), bytes).unwrap();
        let target = |path: &std::path::Path| ReviewTarget::Directory {
            path: path.to_string_lossy().into_owned(),
        };
        let context = service.resolve_target(&target(&directory)).await.unwrap();
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
        let directory_scope = ReviewScope::Review {
            review_id: session.id,
        };
        let before = service
            .get_scope_conversation(&directory_scope)
            .await
            .unwrap()
            .unwrap();
        let directory_comment = service
            .add_comment(
                &session.id,
                &actor,
                "Preserve directory feedback",
                Anchor::Line {
                    path: "PLAN.md".into(),
                    start: 2,
                    end: 2,
                    side: "new".into(),
                },
                Some(evidence(bytes)),
                Some(
                    &MutationGuard::new(
                        Some(before.context.revision),
                        "backend-directory-note".into(),
                    )
                    .in_scope(directory_scope.clone()),
                ),
            )
            .await
            .unwrap();
        let version = service
            .get_scope_conversation(&directory_scope)
            .await
            .unwrap()
            .unwrap()
            .context
            .revision;
        std::fs::rename(&directory, &renamed_directory).unwrap();
        assert_eq!(
            service
                .refresh_locations(
                    &directory_scope,
                    &actor,
                    &[],
                    &MutationGuard::new(Some(version), "backend-discover-root".into()),
                )
                .await
                .unwrap(),
            version + 1
        );
        assert_eq!(
            service.get_review(&session.id).await.unwrap().target,
            target(&renamed_directory)
        );
        let moved = service
            .get_scope_conversation(&directory_scope)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(moved.comments[0].id, directory_comment.id);
        assert_eq!(
            moved.threads[0].current_path.as_deref(),
            renamed_directory.join("PLAN.md").to_str()
        );
        let restarted = common::service(&url).await.unwrap();
        assert_eq!(
            restarted.get_review(&session.id).await.unwrap().target,
            target(&renamed_directory)
        );
        restarted
            .complete(
                &context.id,
                &actor,
                Completion::default(),
                "complete-moved",
                Some(&session.id),
                None,
            )
            .await
            .unwrap();
        let completed_destination = root.join("completed-directory");
        std::fs::rename(&renamed_directory, &completed_destination).unwrap();
        assert_eq!(
            restarted.get_review(&session.id).await.unwrap().target,
            target(&completed_destination)
        );
    }
}
