//! Exercise snapshots through the existing review and notes services and real databases.
mod common;
use lgtm_backend::{models::*, services::ReviewService};
use std::{fs, path::Path, process::Command};

// fixtures --------------------------------------------------------------------

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["-c", "core.hooksPath=/dev/null"])
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}

fn project(root: &Path) -> String {
    fs::create_dir_all(root).unwrap();
    git(root, &["init", "-q", "-b", "main"]);
    git(root, &["config", "user.name", "Snapshot fixture"]);
    git(root, &["config", "user.email", "snapshot@example.test"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    fs::write(root.join("code.txt"), "before\noriginal\nafter\n").unwrap();
    fs::write(root.join(".gitignore"), "notes/\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    git(root, &["rev-parse", "HEAD"])
}

async fn note(service: &ReviewService, file: &Path, actor: &Actor) -> (ReviewScope, Comment) {
    let scope = ReviewScope::Notes {
        path: file.to_string_lossy().into_owned(),
    };
    let writer = service.resolve_write_scope(&scope, actor).await.unwrap();
    let bytes = fs::read(file).unwrap();
    let comment = service
        .add_comment(
            &writer.id,
            actor,
            "Preserve this note",
            Anchor::Line {
                path: file.file_name().unwrap().to_string_lossy().into_owned(),
                start: 2,
                end: 2,
                side: "new".into(),
            },
            Some(
                AnchorEvidence::capture(
                    AnchorSource::LocalFile {
                        content_hash: AnchorSource::hash(&bytes),
                    },
                    &bytes,
                    2,
                    2,
                )
                .unwrap(),
            ),
            Some(&MutationGuard::new(Some(0), "save-note".into()).in_scope(scope.clone())),
        )
        .await
        .unwrap();
    (scope, comment)
}

// tests -----------------------------------------------------------------------

#[cfg(unix)]
#[tokio::test]
async fn should_reject_a_working_snapshot_that_crosses_a_replaced_directory_symlink() {
    for (service, _, temp) in common::databases().await {
        let root = temp.path().join("project");
        project(&root);
        fs::create_dir(root.join("src")).unwrap();
        fs::write(root.join("src/file.txt"), "tracked\n").unwrap();
        git(&root, &["add", "."]);
        git(&root, &["commit", "-qm", "tracked directory"]);
        let head = git(&root, &["rev-parse", "HEAD"]);
        fs::rename(root.join("src"), temp.path().join("outside")).unwrap();
        std::os::unix::fs::symlink(temp.path().join("outside"), root.join("src")).unwrap();
        let actor = service
            .auth()
            .ensure_human("Reviewer", "boundary@example.test")
            .await
            .unwrap();
        let context = service
            .resolve_target(&ReviewTarget::LocalGit {
                path: root.to_string_lossy().into_owned(),
                branch: None,
            })
            .await
            .unwrap();

        let result = service
            .start_session(
                &context,
                &actor,
                CommitContext {
                    base: Some(head.clone()),
                    head: Some(head),
                },
                ComparisonKind::Working,
                None,
            )
            .await;

        assert!(result.is_err());
        assert!(
            service
                .get_sessions(&SessionFilter::default())
                .await
                .unwrap()
                .is_empty()
        );
    }
}

#[tokio::test]
async fn should_snapshot_local_review_and_ignored_notes_without_changing_the_checkout() {
    for (service, url, temp) in common::databases().await {
        let root = temp.path().join("project");
        let base = project(&root);
        fs::write(root.join("code.txt"), "before\nstaged\nafter\n").unwrap();
        git(&root, &["add", "code.txt"]);
        fs::write(root.join("code.txt"), "before\nworking\nafter\n").unwrap();
        fs::create_dir(root.join("notes")).unwrap();
        let file = root.join("notes/PLAN.md");
        let note_bytes = b"before\nignored note\nafter\n";
        fs::write(&file, note_bytes).unwrap();
        let index = fs::read(root.join(".git/index")).unwrap();
        let refs = git(&root, &["show-ref"]);
        let actor = service
            .auth()
            .ensure_human("Reviewer", "snapshot@example.test")
            .await
            .unwrap();
        let context = service
            .resolve_target(&ReviewTarget::LocalGit {
                path: root.to_string_lossy().into_owned(),
                branch: None,
            })
            .await
            .unwrap();
        let session = service
            .start_session(
                &context,
                &actor,
                CommitContext {
                    base: Some(base.clone()),
                    head: Some(base.clone()),
                },
                ComparisonKind::Staged,
                None,
            )
            .await
            .unwrap();
        let scope = ReviewScope::Review {
            review_id: session.id,
        };
        let bytes = b"before\nstaged\nafter\n";
        let oid = git(&root, &["rev-parse", ":code.txt"]);
        let evidence = AnchorEvidence::capture(
            AnchorSource::IndexFile {
                blob_oid: oid,
                content_hash: AnchorSource::hash(bytes),
            },
            bytes,
            2,
            2,
        )
        .unwrap();
        let anchor = Anchor::Line {
            path: "code.txt".into(),
            start: 2,
            end: 2,
            side: "new".into(),
        };
        let guard = MutationGuard::new(Some(1), "save-review".into()).in_scope(scope.clone());

        let comment = service
            .add_comment(
                &session.id,
                &actor,
                "Review staged content",
                anchor.clone(),
                Some(evidence.clone()),
                Some(&guard),
            )
            .await
            .unwrap();
        let retry = service
            .add_comment(
                &session.id,
                &actor,
                "Review staged content",
                anchor,
                Some(evidence),
                Some(&guard),
            )
            .await
            .unwrap();
        assert_eq!(comment.id, retry.id);
        let (note_scope, note) = note(&service, &file, &actor).await;

        let (snapshot, original) = service
            .get_thread_source(&scope, &comment.thread_id)
            .await
            .unwrap();
        assert_eq!(original.as_bytes(), bytes);
        let (note_snapshot, original_note) = service
            .get_thread_source(&note_scope, &note.thread_id)
            .await
            .unwrap();
        assert_eq!(original_note.as_bytes(), note_bytes);
        assert_eq!(snapshot.project_slug, note_snapshot.project_slug);
        let git_dir = temp.path().join("local").join(&snapshot.project_slug);
        assert_eq!(
            git(&git_dir, &["rev-parse", "--is-bare-repository"]),
            "true"
        );
        assert_eq!(
            git(
                &git_dir,
                &[
                    "show",
                    &format!("refs/lgtm/reviews/{}:code.txt", session.id)
                ]
            ),
            "before\nstaged\nafter"
        );
        assert_eq!(git(&root, &["rev-parse", "HEAD"]), base);
        assert_eq!(git(&root, &["show-ref"]), refs);
        assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
        assert_eq!(
            fs::read(root.join("code.txt")).unwrap(),
            b"before\nworking\nafter\n"
        );
        assert!(
            service
                .get_thread_source(&scope, &note.thread_id)
                .await
                .is_err()
        );

        fs::write(&file, "rewritten note\n").unwrap();
        fs::remove_file(root.join("code.txt")).unwrap();
        let reopened = common::service(&url)
            .await
            .unwrap()
            .with_snapshot_root(temp.path().join("local"));
        assert_eq!(
            reopened
                .get_thread_source(&scope, &comment.thread_id)
                .await
                .unwrap()
                .1
                .as_bytes(),
            bytes
        );
        assert_eq!(
            reopened
                .get_thread_source(&note_scope, &note.thread_id)
                .await
                .unwrap()
                .1
                .as_bytes(),
            note_bytes
        );
        git(&git_dir, &["gc", "--prune=now"]);
        assert_eq!(
            reopened
                .get_thread_source(&note_scope, &note.thread_id)
                .await
                .unwrap()
                .1
                .as_bytes(),
            note_bytes
        );
    }
}

#[tokio::test]
async fn should_keep_unsquashed_commits_when_a_working_review_snapshots_the_project() {
    for (service, _, temp) in common::databases().await {
        let root = temp.path().join("project");
        let base = project(&root);
        fs::write(root.join("code.txt"), "intermediate\n").unwrap();
        git(&root, &["commit", "-qam", "intermediate"]);
        let intermediate = git(&root, &["rev-parse", "HEAD"]);
        fs::write(root.join("code.txt"), "final\n").unwrap();
        git(&root, &["commit", "-qam", "final"]);
        let head = git(&root, &["rev-parse", "HEAD"]);
        fs::write(root.join("code.txt"), "saved but uncommitted\n").unwrap();
        let actor = service
            .auth()
            .ensure_human("Reviewer", "squash@example.test")
            .await
            .unwrap();
        let context = service
            .resolve_target(&ReviewTarget::LocalGit {
                path: root.to_string_lossy().into_owned(),
                branch: None,
            })
            .await
            .unwrap();

        let session = service
            .start_session(
                &context,
                &actor,
                CommitContext {
                    base: Some(base.clone()),
                    head: Some(head),
                },
                ComparisonKind::Working,
                None,
            )
            .await
            .unwrap();
        git(&root, &["reset", "--soft", &base]);
        git(&root, &["commit", "-qm", "squash"]);
        git(&root, &["reflog", "expire", "--expire=now", "--all"]);
        git(&root, &["gc", "--prune=now"]);

        let git_dir = fs::read_dir(temp.path().join("local"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        assert_eq!(
            git(&git_dir, &["show", &format!("{intermediate}:code.txt")]),
            "intermediate"
        );
        assert_eq!(
            git(
                &git_dir,
                &[
                    "show",
                    &format!("refs/lgtm/reviews/{}:code.txt", session.id)
                ]
            ),
            "saved but uncommitted"
        );
    }
}
