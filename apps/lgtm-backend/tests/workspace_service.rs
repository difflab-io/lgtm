mod common;
use lgtm_backend::{models::*, services::workspace::get_workspace};

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
            .start_session(&context, &alice, CommitContext::default())
            .await
            .unwrap();
        let anchor = Anchor::Line {
            path: files[0].path.clone(),
            start: 1,
            end: 2,
            side: "new".into(),
            commit: files[0].content_hash.clone(),
        };
        let note = service
            .add_comment(&session.id, &alice, "Clarify this", anchor.clone())
            .await
            .unwrap();
        assert!(
            service
                .edit_comment(&session.id, &note.id, &bob, "No")
                .await
                .is_err()
        );
        assert_eq!(
            service
                .edit_comment(&session.id, &note.id, &alice, "Updated")
                .await
                .unwrap()
                .body,
            "Updated"
        );
        let reply = service
            .reply(&session.id, &note.thread_id, &bob, "Reply")
            .await
            .unwrap();
        assert!(
            service
                .delete_comment(&session.id, &note.id, &alice)
                .await
                .is_err()
        );
        service
            .delete_comment(&session.id, &reply.id, &bob)
            .await
            .unwrap();
        service
            .delete_comment(&session.id, &note.id, &alice)
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
                .add_comment(&session.id, &alice, "Stale", anchor)
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
async fn should_promote_notes_atomically_with_attribution_and_deleted_target_receipts() {
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
        let target = service
            .ensure_context(
                ContextKey::Local {
                    location: LocalLocation::Branch {
                        git_dir: temp.path().join(".git").to_str().unwrap().into(),
                        branch: "refs/heads/main".into(),
                    },
                },
                None,
            )
            .await
            .unwrap();
        let notes = service
            .start_session(&source, &alice, CommitContext::default())
            .await
            .unwrap();
        let review = service
            .start_session(&target, &bob, CommitContext::default())
            .await
            .unwrap();
        let note = service
            .add_comment(&notes.id, &alice, "Keep attribution", Anchor::General)
            .await
            .unwrap();
        let selected = [note.id];
        let anchors = [Anchor::General];
        let promote = || {
            service.promote_notes(
                &source.id,
                &review.id,
                &bob,
                &selected,
                &anchors,
                "promote-1",
            )
        };
        // A later invalid note rolls the earlier insertion back.
        assert!(
            service
                .promote_notes(
                    &source.id,
                    &review.id,
                    &bob,
                    &[note.id, uuid::Uuid::new_v4()],
                    &[Anchor::General, Anchor::General],
                    "bad"
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
        service
            .delete_comment(&review.id, &first[0].id, &alice)
            .await
            .unwrap();
        assert!(promote().await.is_err());
        let receipt = service
            .database()
            .promotions
            .get_source_target(&note.id, &review.id)
            .await
            .unwrap()
            .unwrap();
        assert!(receipt.target_comment_id.is_none());
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
            .start_session(&context, &actor, CommitContext::default())
            .await
            .unwrap();
        let note = service
            .add_comment(
                &session.id,
                &actor,
                "Original",
                Anchor::Line {
                    path: file.path,
                    start: 2,
                    end: 2,
                    side: "new".into(),
                    commit: file.content_hash.clone(),
                },
            )
            .await
            .unwrap();
        std::fs::write(root.join("PLAN.md"), "new first\nfirst\nsecond\n").unwrap();
        let reopened = common::service(&url).await.unwrap();
        let conversation = reopened.get_conversation(&context.id, true).await.unwrap();
        assert!(!conversation.threads[0].outdated);
        assert!(matches!(
            &conversation.threads[0].anchor,
            Anchor::Line {
                start: 3,
                end: 3,
                ..
            }
        ));
        std::fs::write(root.join("PLAN.md"), "second\nfirst\nsecond\n").unwrap();
        let reopened = common::service(&url).await.unwrap();
        let ambiguous = reopened.get_conversation(&context.id, true).await.unwrap();
        assert!(ambiguous.threads[0].outdated);
        assert!(matches!(&ambiguous.threads[0].anchor, Anchor::File { path } if path == "PLAN.md"));
        std::fs::write(root.join("PLAN.md"), "no original text\n").unwrap();
        let deleted = reopened.get_conversation(&context.id, true).await.unwrap();
        assert!(deleted.threads[0].outdated);
        assert!(matches!(&deleted.threads[0].anchor, Anchor::File { .. }));
        std::fs::remove_file(root.join("PLAN.md")).unwrap();
        let missing = reopened.get_conversation(&context.id, true).await.unwrap();
        assert!(missing.threads[0].outdated);
        let stored = reopened
            .database()
            .comments
            .get_thread(&context.id, &note.thread_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.selected_text.as_deref(), Some("second"));
        assert_eq!(stored.start_line, Some(2));
        assert_eq!(
            stored.reviewed_commit.as_deref(),
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
