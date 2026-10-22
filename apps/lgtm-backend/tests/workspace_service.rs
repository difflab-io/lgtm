mod common;
use lgtm_backend::{models::*, services::workspace::get_workspace};

// fixtures --------------------------------------------------------------------
fn placement(thread: &Thread, content: Option<&str>) -> AnchorPlacement {
    let source = content.map(|text| AnchorSource::LocalFile {
        content_hash: AnchorSource::hash(text.as_bytes()),
    });
    thread.anchor.get_placement(
        thread.evidence.as_ref(),
        "PLAN.md",
        source.as_ref(),
        content,
        true,
    )
}

// tests -----------------------------------------------------------------------
#[tokio::test]
async fn should_anchor_ignored_non_git_files_and_guard_owned_draft_mutations() {
    for (service, _, temp) in common::databases().await {
        let root = temp.path().join("notes");
        std::fs::create_dir_all(root.join(".ignored")).unwrap();
        std::fs::write(root.join(".ignored/plan.md"), "one\ntwo\n").unwrap();
        let context = service
            .ensure_context(
                ContextKey::Local {
                    location: LocalLocation::Notes {
                        path: root.to_str().unwrap().into(),
                    },
                },
                None,
            )
            .await
            .unwrap();
        let (_, files) = get_workspace(&context, "").unwrap();
        assert_eq!(files.len(), 1);
        let alice = service
            .auth()
            .ensure_human("Alice", "alice@example.com")
            .await
            .unwrap();
        let bob = service
            .auth()
            .ensure_human("Bob", "bob@example.com")
            .await
            .unwrap();
        let session = service
            .start_session(
                &context,
                &alice,
                CommitContext::default(),
                crate::common::comparison(&context),
                None,
            )
            .await
            .unwrap();
        let anchor = Anchor::Line {
            path: files[0].path.clone(),
            start: 1,
            end: 2,
            side: "new".into(),
        };
        let note = service
            .add_comment(
                &session.id,
                &alice,
                "Clarify this",
                anchor.clone(),
                Some(
                    AnchorEvidence::capture(
                        AnchorSource::LocalFile {
                            content_hash: files[0].content_hash.clone(),
                        },
                        files[0].content.as_bytes(),
                        1,
                        2,
                    )
                    .unwrap(),
                ),
                None,
            )
            .await
            .unwrap();
        assert!(
            service
                .edit_comment(&session.id, &note.id, &bob, "No", None)
                .await
                .is_err()
        );
        assert_eq!(
            service
                .edit_comment(&session.id, &note.id, &alice, "Updated", None)
                .await
                .unwrap()
                .body,
            "Updated"
        );
        let reply = service
            .reply(&session.id, &note.thread_id, &bob, "Reply", None)
            .await
            .unwrap();
        assert!(
            service
                .delete_comment(&session.id, &note.id, &alice, None)
                .await
                .is_err()
        );
        service
            .delete_comment(&session.id, &reply.id, &bob, None)
            .await
            .unwrap();
        service
            .delete_comment(&session.id, &note.id, &alice, None)
            .await
            .unwrap();
        assert!(
            service
                .get_conversation(&context.id, true)
                .await
                .unwrap()
                .threads
                .is_empty()
        );
        std::fs::write(root.join(".ignored/plan.md"), "changed\n").unwrap();
        assert!(
            service
                .add_comment(&session.id, &alice, "Stale", anchor, None, None)
                .await
                .is_err()
        );
    }
}

#[cfg(unix)]
#[tokio::test]
async fn should_follow_an_explicit_symlink_root_but_reject_nested_escape() {
    for (service, _, temp) in common::databases().await {
        let root = temp.path().join("physical");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("PLAN.md"), "Plan").unwrap();
        std::os::unix::fs::symlink(&root, temp.path().join("logical")).unwrap();
        std::os::unix::fs::symlink(temp.path(), root.join("escape")).unwrap();
        let c = service
            .ensure_context(
                ContextKey::Local {
                    location: LocalLocation::Notes {
                        path: temp.path().join("logical").to_str().unwrap().into(),
                    },
                },
                None,
            )
            .await
            .unwrap();
        assert_eq!(get_workspace(&c, "").unwrap().1.len(), 1);
        assert!(get_workspace(&c, "escape").is_err());
    }
}

#[tokio::test]
async fn should_promote_notes_atomically_with_attribution_and_safe_retries_after_deletion() {
    for (service, _, temp) in common::databases().await {
        let alice = service
            .auth()
            .ensure_human("Alice", "alice@example.com")
            .await
            .unwrap();
        let bob = service
            .auth()
            .ensure_human("Bob", "bob@example.com")
            .await
            .unwrap();
        let source = service
            .ensure_context(
                ContextKey::Local {
                    location: LocalLocation::Notes {
                        path: temp.path().to_str().unwrap().into(),
                    },
                },
                None,
            )
            .await
            .unwrap();
        let output = std::process::Command::new("git")
            .args(["init", "-q", "-b", "main"])
            .current_dir(temp.path())
            .output()
            .unwrap();
        assert!(output.status.success());
        let target = service
            .ensure_context(
                ContextKey::Local {
                    location: LocalLocation::Branch {
                        git_dir: temp
                            .path()
                            .join(".git")
                            .canonicalize()
                            .unwrap()
                            .to_str()
                            .unwrap()
                            .into(),
                        branch: "refs/heads/main".into(),
                    },
                },
                Some(temp.path().to_str().unwrap()),
            )
            .await
            .unwrap();
        let notes = service
            .start_session(
                &source,
                &alice,
                CommitContext::default(),
                crate::common::comparison(&source),
                None,
            )
            .await
            .unwrap();
        let review = service
            .start_session(
                &target,
                &bob,
                CommitContext::default(),
                crate::common::comparison(&target),
                None,
            )
            .await
            .unwrap();
        let note = service
            .add_comment(
                &notes.id,
                &alice,
                "Keep attribution",
                Anchor::General,
                None,
                None,
            )
            .await
            .unwrap();
        let selected = [note.id];
        let anchors = [Anchor::General];
        let promote = || {
            service.promote_notes(
                &source.id,
                &review.id,
                &bob,
                NotePromotion {
                    comment_ids: selected.to_vec(),
                    anchors: anchors.to_vec(),
                    evidence: vec![],
                    expected_source_version: None,
                    guard: MutationGuard::new(None, "promote-1".into()),
                },
            )
        };
        // A later invalid note rolls the earlier insertion back.
        assert!(
            service
                .promote_notes(
                    &source.id,
                    &review.id,
                    &bob,
                    NotePromotion {
                        comment_ids: [note.id, uuid::Uuid::new_v4()].to_vec(),
                        anchors: [Anchor::General, Anchor::General].to_vec(),
                        evidence: vec![],
                        expected_source_version: None,
                        guard: MutationGuard::new(None, "bad".into())
                    }
                )
                .await
                .is_err()
        );
        assert!(
            service
                .get_conversation(&target.id, true)
                .await
                .unwrap()
                .comments
                .is_empty()
        );
        let first = promote().await.unwrap();
        assert_eq!(first[0].author.id, alice.id);
        assert_eq!(promote().await.unwrap()[0].id, first[0].id);
        assert_eq!(
            service
                .get_comment(&source.id, &note.id)
                .await
                .unwrap()
                .body,
            "Keep attribution"
        );
        assert_eq!(first[0].owner_id, Some(bob.id));
        assert!(
            service
                .delete_comment(&review.id, &first[0].id, &alice, None)
                .await
                .is_err(),
            "the source author does not own another actor's promoted draft"
        );
        let edited = service
            .edit_comment(
                &review.id,
                &first[0].id,
                &bob,
                "Promoter can revise this draft",
                None,
            )
            .await
            .unwrap();
        assert_eq!(edited.author.id, alice.id);
        service
            .delete_comment(&review.id, &first[0].id, &bob, None)
            .await
            .unwrap();
        assert_eq!(
            promote().await.unwrap()[0].id,
            first[0].id,
            "retry returns the accepted result without resurrecting a deleted comment"
        );
        assert!(
            service
                .get_conversation(&target.id, true)
                .await
                .unwrap()
                .comments
                .is_empty()
        );
        let second = service
            .promote_notes(
                &source.id,
                &review.id,
                &bob,
                NotePromotion {
                    comment_ids: selected.to_vec(),
                    anchors: anchors.to_vec(),
                    evidence: vec![],
                    expected_source_version: None,
                    guard: MutationGuard::new(None, "promote-again".into()),
                },
            )
            .await
            .unwrap();
        assert_ne!(second[0].id, first[0].id);
        assert_eq!(second[0].source_note_id, Some(note.id));
        assert_eq!(second[0].author.id, alice.id);
    }
}

#[tokio::test]
async fn should_select_working_staged_and_revision_content_without_checkout() {
    use lgtm_backend::services::workspace::get_review_workspace;
    for (service, _, temp) in common::databases().await {
        let root = temp.path().join("repo");
        std::fs::create_dir(&root).unwrap();
        let git = |args: &[&str]| {
            let output = std::process::Command::new("git")
                .arg("-C")
                .arg(&root)
                .args(args)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8_lossy(&output.stdout).trim().to_owned()
        };
        git(&["init", "-b", "main"]);
        git(&["config", "user.email", "test@example.com"]);
        git(&["config", "user.name", "Test"]);
        std::fs::write(root.join("file.txt"), "base\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "initial"]);
        let base = git(&["rev-parse", "HEAD"]);
        std::fs::write(root.join("file.txt"), "index\n").unwrap();
        git(&["add", "."]);
        std::fs::write(root.join("file.txt"), "working\n").unwrap();
        let c = service
            .ensure_context(
                ContextKey::Local {
                    location: LocalLocation::Branch {
                        git_dir: root.join(".git").to_str().unwrap().into(),
                        branch: "refs/heads/main".into(),
                    },
                },
                Some(root.to_str().unwrap()),
            )
            .await
            .unwrap();
        let staged = get_review_workspace(&c, "", "staged", &base, "").unwrap().1;
        assert_eq!(staged[0].0.content, "index\n");
        assert_eq!(staged[0].1, "base\n");
        let working = get_review_workspace(&c, "", "working", &base, "")
            .unwrap()
            .1;
        assert_eq!(working[0].0.content, "working\n");
        git(&["commit", "-m", "index commit"]);
        let head = git(&["rev-parse", "HEAD"]);
        let revisions = get_review_workspace(&c, "", "revisions", &base, &head)
            .unwrap()
            .1;
        assert_eq!(revisions[0].0.content, "index\n");
        assert_eq!(
            std::fs::read_to_string(root.join("file.txt")).unwrap(),
            "working\n"
        );
    }
}

#[tokio::test]
async fn should_relocate_unique_text_and_fall_back_for_ambiguous_or_deleted_text_after_restart() {
    for (service, url, temp) in common::databases().await {
        let root = temp.path().join("notes");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("PLAN.md"), "first\nsecond\n").unwrap();
        let context = service
            .ensure_context(
                ContextKey::Local {
                    location: LocalLocation::Notes {
                        path: root.to_str().unwrap().into(),
                    },
                },
                None,
            )
            .await
            .unwrap();
        let file = get_workspace(&context, "").unwrap().1.remove(0);
        let actor = service
            .auth()
            .ensure_human("Alice", "alice@example.com")
            .await
            .unwrap();
        let session = service
            .start_session(
                &context,
                &actor,
                CommitContext::default(),
                crate::common::comparison(&context),
                None,
            )
            .await
            .unwrap();
        let note = service
            .add_comment(
                &session.id,
                &actor,
                "Original",
                Anchor::Line {
                    path: file.path.clone(),
                    start: 2,
                    end: 2,
                    side: "new".into(),
                },
                Some(
                    AnchorEvidence::capture(
                        AnchorSource::LocalFile {
                            content_hash: file.content_hash.clone(),
                        },
                        file.content.as_bytes(),
                        2,
                        2,
                    )
                    .unwrap(),
                ),
                None,
            )
            .await
            .unwrap();
        std::fs::write(root.join("PLAN.md"), "new first\nfirst\nsecond\n").unwrap();
        let reopened = common::service(&url).await.unwrap();
        let conversation = reopened.get_conversation(&context.id, true).await.unwrap();
        assert!(
            matches!(
                &conversation.threads[0].anchor,
                Anchor::Line {
                    start: 2,
                    end: 2,
                    ..
                }
            ),
            "review reads preserve the original anchor"
        );
        assert!(matches!(
            placement(&conversation.threads[0], Some("new first\nfirst\nsecond\n")),
            AnchorPlacement::Relocated {
                location: Anchor::Line {
                    start: 3,
                    end: 3,
                    ..
                },
                ..
            }
        ));
        std::fs::write(root.join("PLAN.md"), "first\nsecond\nfirst\nsecond\n").unwrap();
        let reopened = common::service(&url).await.unwrap();
        let ambiguous = reopened.get_conversation(&context.id, true).await.unwrap();
        assert!(matches!(
            placement(
                &ambiguous.threads[0],
                Some("first\nsecond\nfirst\nsecond\n")
            ),
            AnchorPlacement::Outdated { .. }
        ));
        assert!(matches!(
            &ambiguous.threads[0].anchor,
            Anchor::Line { start: 2, .. }
        ));
        std::fs::write(root.join("PLAN.md"), "no original text\n").unwrap();
        let deleted = reopened.get_conversation(&context.id, true).await.unwrap();
        assert!(matches!(
            placement(&deleted.threads[0], Some("no original text\n")),
            AnchorPlacement::Outdated { .. }
        ));
        assert!(matches!(
            &deleted.threads[0].anchor,
            Anchor::Line { start: 2, .. }
        ));
        std::fs::remove_file(root.join("PLAN.md")).unwrap();
        let version = reopened
            .get_context_by_id(&context.id)
            .await
            .unwrap()
            .revision;
        reopened
            .refresh_locations(
                &ReviewScope::Review {
                    review_id: session.id,
                },
                &actor,
                &[SourceChange::Deleted {
                    thread_id: note.thread_id,
                    path: "PLAN.md".into(),
                }],
                &MutationGuard::new(Some(version), "confirm-deletion".into()),
            )
            .await
            .unwrap();
        let missing = reopened.get_conversation(&context.id, true).await.unwrap();
        assert_eq!(
            missing.threads[0].orphan_reason,
            Some(OrphanReason::FileDeleted)
        );
        assert!(matches!(
            placement(&missing.threads[0], None),
            AnchorPlacement::Orphaned { .. }
        ));
        std::fs::write(root.join("PLAN.md"), "first\nsecond\n").unwrap();
        let version = reopened
            .get_context_by_id(&context.id)
            .await
            .unwrap()
            .revision;
        reopened
            .refresh_locations(
                &ReviewScope::Review {
                    review_id: session.id,
                },
                &actor,
                &[SourceChange::Restored {
                    thread_id: note.thread_id,
                    path: "PLAN.md".into(),
                    source: AnchorSource::LocalFile {
                        content_hash: file.content_hash.clone(),
                    },
                }],
                &MutationGuard::new(Some(version), "restore-matching-content".into()),
            )
            .await
            .unwrap();
        let restored = reopened.get_conversation(&context.id, true).await.unwrap();
        assert!(restored.threads[0].orphan_reason.is_none());
        assert_eq!(restored.threads[0].id, note.thread_id);
        assert!(matches!(
            placement(&restored.threads[0], Some("first\nsecond\n")),
            AnchorPlacement::Exact { .. }
        ));
        let stored = reopened
            .database()
            .comments
            .get_thread(&context.id, &note.thread_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            stored.source_evidence.as_ref().unwrap()["selected_text"].as_str(),
            Some("second")
        );
        assert_eq!(stored.start_line, Some(2));
        assert_eq!(
            stored.source_evidence.as_ref().unwrap()["source"]["content_hash"].as_str(),
            Some(file.content_hash.as_str())
        );
    }
}

#[tokio::test]
async fn should_keep_a_single_file_notes_context_scoped_to_that_file() {
    for (service, _, temp) in common::databases().await {
        let file = temp.path().join("note.txt");
        std::fs::write(&file, "Allowed\n").unwrap();
        std::fs::write(temp.path().join("sibling.txt"), "Outside scope\n").unwrap();
        let context = service
            .ensure_context(
                ContextKey::Local {
                    location: LocalLocation::Notes {
                        path: file.to_str().unwrap().into(),
                    },
                },
                None,
            )
            .await
            .unwrap();
        assert_eq!(
            get_workspace(&context, "").unwrap().1[0].content,
            "Allowed\n"
        );
        assert_eq!(get_workspace(&context, "note.txt").unwrap().1.len(), 1);
        assert!(get_workspace(&context, "sibling.txt").is_err());
    }
}

#[test]
fn should_capture_index_bytes_separately_from_working_and_committed_content() {
    use lgtm_backend::{database::types::SourceKind, services::workspace::get_source};
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let git = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    };
    git(&["init", "-q"]);
    let file = root.join("file[1].txt");
    std::fs::write(&file, b"original\r\nselected\r\n").unwrap();
    git(&["--literal-pathspecs", "add", "--", "file[1].txt"]);
    git(&[
        "-c",
        "user.name=E2E",
        "-c",
        "user.email=e2e@example.test",
        "commit",
        "-qm",
        "initial",
    ]);
    let head = git(&["rev-parse", "HEAD"]);
    std::fs::write(&file, b"staged\r\nselected").unwrap();
    git(&["--literal-pathspecs", "add", "--", "file[1].txt"]);
    std::fs::write(&file, b"working\nselected\n").unwrap();
    let before = git(&["status", "--porcelain"]);
    let (index, bytes) = get_source(root, "file[1].txt", SourceKind::IndexFile, None).unwrap();
    assert_eq!(bytes, b"staged\r\nselected");
    assert!(
        matches!(&index, AnchorSource::IndexFile { blob_oid, .. } if blob_oid == &git(&["rev-parse", ":file[1].txt"]))
    );
    let evidence = AnchorEvidence::capture(index.clone(), &bytes, 2, 2).unwrap();
    assert_eq!(evidence.selected_text, "selected");
    assert_eq!(evidence.context_before, "staged");
    let (working, bytes) = get_source(root, "file[1].txt", SourceKind::WorkingFile, None).unwrap();
    assert_eq!(bytes, b"working\nselected\n");
    assert_ne!(working.content_hash(), index.content_hash());
    let (committed, bytes) =
        get_source(root, "file[1].txt", SourceKind::Commit, Some(&head)).unwrap();
    assert_eq!(bytes, b"original\r\nselected\r\n");
    assert!(matches!(committed, AnchorSource::Commit { sha, .. } if sha == head));
    assert_eq!(
        git(&["status", "--porcelain"]),
        before,
        "source reads must not change the checkout or index"
    );
    assert!(get_source(root, "../outside", SourceKind::WorkingFile, None).is_err());
}

#[test]
fn should_distinguish_line_endings_and_final_newline_in_source_identity() {
    let first = AnchorSource::hash(b"line\r\n");
    assert_ne!(first, AnchorSource::hash(b"line\n"));
    assert_ne!(first, AnchorSource::hash(b"line"));
    let source = AnchorSource::LocalFile {
        content_hash: first,
    };
    assert!(AnchorEvidence::capture(source.clone(), b"line\r\n", 1, 1).is_ok());
    assert!(AnchorEvidence::capture(source.clone(), b"line\n", 1, 1).is_err());
    assert!(AnchorEvidence::capture(source, b"line\r\n", 1, 2).is_err());
}
