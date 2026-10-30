mod common;
use lgtm_backend::{
    models::{ReviewComparison, ReviewTarget},
    services::SnapshotService,
};
use std::path::Path;

// fixtures --------------------------------------------------------------------
fn get_bytes(root: &Path, args: &[&str]) -> std::io::Result<Vec<u8>> {
    let output = std::process::Command::new("git")
        .arg("--literal-pathspecs")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()?;
    if !output.status.success() {
        return Err(std::io::Error::other(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }
    Ok(output.stdout)
}

fn command(root: &Path, args: &[&str]) -> String {
    String::from_utf8(get_bytes(root, args).unwrap())
        .unwrap()
        .trim()
        .into()
}

fn repository() -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    command(dir.path(), &["init", "-q"]);
    command(dir.path(), &["config", "user.name", "Fixture"]);
    command(dir.path(), &["config", "user.email", "fixture@example.com"]);
    std::fs::write(dir.path().join("source.txt"), "committed\n").unwrap();
    command(dir.path(), &["add", "."]);
    command(dir.path(), &["commit", "-qm", "Initial"]);
    let head = command(dir.path(), &["rev-parse", "HEAD"]);
    (dir, head)
}

fn target(root: &Path) -> ReviewTarget {
    ReviewTarget::LocalGit {
        path: root.to_string_lossy().into_owned(),
        branch: None,
    }
}

// tests -----------------------------------------------------------------------
#[tokio::test]
async fn should_capture_working_and_index_without_mutating_user_git_state() {
    let (source, head) = repository();
    let storage = tempfile::tempdir().unwrap();
    let service = SnapshotService::new(storage.path().join("snapshots"));
    std::fs::write(source.path().join("source.txt"), "staged\n").unwrap();
    command(source.path(), &["add", "."]);
    std::fs::write(source.path().join("source.txt"), "working\n").unwrap();
    std::fs::write(source.path().join("new.txt"), "untracked\n").unwrap();
    let index_before = std::fs::read(source.path().join(".git/index")).unwrap();
    let staged = service
        .capture(
            target(source.path()),
            ReviewComparison::Staged {
                base_commit: head.clone(),
            },
        )
        .await
        .unwrap();
    let working = service
        .capture(
            target(source.path()),
            ReviewComparison::Working {
                base_commit: head.clone(),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        command(
            Path::new(&staged.root),
            &["show", &format!("{}:source.txt", staged.head_commit)]
        ),
        "staged"
    );
    assert_eq!(
        command(
            Path::new(&working.root),
            &["show", &format!("{}:source.txt", working.head_commit)]
        ),
        "working"
    );
    assert_eq!(
        command(
            Path::new(&working.root),
            &["show", &format!("{}:new.txt", working.head_commit)]
        ),
        "untracked"
    );
    assert_eq!(
        index_before,
        std::fs::read(source.path().join(".git/index")).unwrap()
    );
    assert_eq!(command(source.path(), &["rev-parse", "HEAD"]), head);
    assert_eq!(
        command(Path::new(&working.root), &["rev-parse", "--show-toplevel"]),
        working.root
    );
}

#[tokio::test]
async fn should_retain_existing_commits_after_source_repository_is_removed() {
    let (source, head) = repository();
    let storage = tempfile::tempdir().unwrap();
    let service = SnapshotService::new(storage.path().join("snapshots"));
    let snapshot = service
        .capture(
            target(source.path()),
            ReviewComparison::Branch {
                base_commit: head.clone(),
                head_commit: head.clone(),
            },
        )
        .await
        .unwrap();
    assert_eq!(snapshot.base_commit, head);
    assert_eq!(snapshot.head_commit, head);
    source.close().unwrap();
    command(Path::new(&snapshot.root), &["gc", "--prune=now"]);
    assert_eq!(
        command(
            Path::new(&snapshot.root),
            &["show", &format!("{}:source.txt", snapshot.head_commit)]
        ),
        "committed"
    );
    assert!(
        !Path::new(&snapshot.root)
            .join(".git/objects/info/alternates")
            .exists()
    );
}

#[tokio::test]
async fn should_reuse_committed_head_for_clean_working_and_staged_snapshots() {
    let (source, head) = repository();
    let storage = tempfile::tempdir().unwrap();
    let service = SnapshotService::new(storage.path().join("snapshots"));
    for comparison in [
        ReviewComparison::Working {
            base_commit: head.clone(),
        },
        ReviewComparison::Staged {
            base_commit: head.clone(),
        },
    ] {
        let snapshot = service
            .capture(target(source.path()), comparison)
            .await
            .unwrap();
        assert_eq!(snapshot.head_commit, head);
    }
}

#[tokio::test]
async fn should_capture_directory_against_empty_baseline() {
    let source = tempfile::tempdir().unwrap();
    let storage = tempfile::tempdir().unwrap();
    std::fs::write(source.path().join("file with spaces.txt"), "saved\n").unwrap();
    let snapshot = SnapshotService::new(storage.path().join("snapshots"))
        .capture(
            ReviewTarget::Directory {
                path: source.path().to_string_lossy().into_owned(),
            },
            ReviewComparison::Directory,
        )
        .await
        .unwrap();
    assert_eq!(
        command(
            Path::new(&snapshot.root),
            &["ls-tree", "--name-only", &snapshot.base_commit]
        ),
        ""
    );
    assert_eq!(
        command(
            Path::new(&snapshot.root),
            &[
                "show",
                &format!("{}:file with spaces.txt", snapshot.head_commit)
            ]
        ),
        "saved"
    );
}

mod review_revisions {
    use super::*;
    use lgtm_backend::models::*;
    use lgtm_backend::services::{AuthService, ReviewService};

    // tests -------------------------------------------------------------------
    #[tokio::test]
    async fn should_keep_original_locations_and_session_identity_across_refresh_and_restart() {
        for (service, url, storage) in crate::common::databases().await {
            let service =
                service.with_snapshots(SnapshotService::new(storage.path().join("snapshots")));
            let (source, head) = repository();
            std::fs::write(source.path().join("source.txt"), "saved selection\n").unwrap();
            let actor = service
                .auth()
                .ensure_human("Reviewer", "reviewer@example.com")
                .await
                .unwrap();
            let (root, git_dir, branch) = lgtm_backend::services::workspace::get_git_location(
                source.path().to_str().unwrap(),
            )
            .await
            .unwrap();
            let context = service
                .ensure_context(
                    ContextKey::Local {
                        location: LocalLocation::Branch { git_dir, branch },
                    },
                    Some(&root),
                )
                .await
                .unwrap();
            let session = service
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
                .await
                .unwrap();
            let first = service.prepare_review(&session.id).await.unwrap();
            let saved = first.snapshot.clone().unwrap();
            let anchor = Anchor::Line {
                path: "source.txt".into(),
                start: 1,
                end: 1,
                side: "new".into(),
            };
            let location = service
                .get_snapshot_location(&session.id, &saved.id, &anchor)
                .await
                .unwrap();
            let comment = service
                .add_comment(
                    &session.id,
                    &actor,
                    "Keep this comment",
                    anchor.clone(),
                    Some(location.clone()),
                    None,
                )
                .await
                .unwrap();
            service
                .reply(
                    &session.id,
                    &comment.thread_id,
                    &actor,
                    "Keep this reply",
                    None,
                )
                .await
                .unwrap();

            std::fs::write(source.path().join("source.txt"), "replacement\n").unwrap();
            let resumed = service.get_review(&session.id).await.unwrap();
            assert_eq!(
                resumed.snapshot.as_ref().unwrap(),
                &saved,
                "reading cannot recapture local source"
            );
            let next = service.capture_snapshot(&resumed, true).await.unwrap();
            assert_ne!(next.id, saved.id);
            assert_eq!(
                service.get_session(&session.id).await.unwrap().id,
                session.id
            );
            let conversation = service.get_conversation(&context.id, true).await.unwrap();
            let thread = &conversation.threads[0];
            assert_eq!(thread.anchor, anchor);
            assert_eq!(thread.evidence.as_ref(), Some(&location));
            assert_eq!(thread.get_display_location(&saved), Some(anchor.clone()));
            assert_eq!(thread.get_display_location(&next), None);
            assert_eq!(conversation.comments.len(), 2);
            let current = service.get_review(&session.id).await.unwrap();
            let refs_before = command(
                Path::new(&next.root),
                &["for-each-ref", "--format=%(refname)", "refs/lgtm/revisions"],
            );
            let repeated = service.capture_snapshot(&current, true).await.unwrap();
            assert_eq!(repeated.id, next.id);
            assert_eq!(
                command(
                    Path::new(&next.root),
                    &["for-each-ref", "--format=%(refname)", "refs/lgtm/revisions"]
                ),
                refs_before,
                "no-op refresh must release unused refs"
            );
            source.close().unwrap();
            service.database().close().await;
            let database = lgtm_backend::database::Database::connect(&url)
                .await
                .unwrap();
            let restarted = ReviewService::new(database.clone(), AuthService::new(database))
                .with_snapshots(SnapshotService::new(storage.path().join("snapshots")));
            let resumed = restarted.get_review(&session.id).await.unwrap();
            assert_eq!(resumed.snapshot.unwrap().id, next.id);
            assert_eq!(resumed.revisions.len(), 2);
            assert_eq!(
                command(
                    Path::new(&saved.root),
                    &["show", &format!("{}:source.txt", saved.head_commit)]
                ),
                "saved selection"
            );
            restarted
                .reply(
                    &session.id,
                    &comment.thread_id,
                    &actor,
                    "Source removal cannot break replies",
                    None,
                )
                .await
                .unwrap();
            restarted.database().close().await;
        }
    }
}

#[tokio::test]
async fn should_preserve_literal_saved_bytes_and_deleted_files_despite_git_attributes() {
    let (source, head) = repository();
    let storage = tempfile::tempdir().unwrap();
    std::fs::write(source.path().join(".gitattributes"), "*.txt text eol=lf\n").unwrap();
    std::fs::remove_file(source.path().join("source.txt")).unwrap();
    let path = "literal [file]\nü.txt";
    let bytes = b"saved\r\nwithout final newline";
    std::fs::write(source.path().join(path), bytes).unwrap();
    let snapshot = SnapshotService::new(storage.path().join("snapshots"))
        .capture(
            target(source.path()),
            ReviewComparison::Working { base_commit: head },
        )
        .await
        .unwrap();
    let saved = get_bytes(
        Path::new(&snapshot.root),
        &["show", &format!("{}:{path}", snapshot.head_commit)],
    )
    .unwrap();
    assert_eq!(saved, bytes);
    assert!(
        get_bytes(
            Path::new(&snapshot.root),
            &[
                "cat-file",
                "-e",
                &format!("{}:source.txt", snapshot.head_commit)
            ]
        )
        .is_err()
    );
}

#[tokio::test]
async fn should_share_repository_identity_between_ssh_and_https_origin_addresses() {
    let (source, head) = repository();
    let storage = tempfile::tempdir().unwrap();
    let service = SnapshotService::new(storage.path().join("snapshots"));
    command(
        source.path(),
        &[
            "remote",
            "add",
            "origin",
            "git@github.com:example/repository.git",
        ],
    );
    let comparison = ReviewComparison::Branch {
        base_commit: head.clone(),
        head_commit: head,
    };
    let ssh = service
        .capture(target(source.path()), comparison.clone())
        .await
        .unwrap();
    command(
        source.path(),
        &[
            "remote",
            "set-url",
            "origin",
            "https://github.com/example/repository",
        ],
    );
    let https = service
        .capture(target(source.path()), comparison)
        .await
        .unwrap();
    assert_eq!(ssh.repository_id, https.repository_id);
    assert_eq!(ssh.root, https.root);
    assert_eq!(ssh.head_commit, https.head_commit);
}

#[cfg(unix)]
#[tokio::test]
async fn should_capture_a_symlinked_directory_root_without_following_nested_escapes() {
    let source = tempfile::tempdir().unwrap();
    let storage = tempfile::tempdir().unwrap();
    let root = source.path().join("physical");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("PLAN.md"), "Selected source\n").unwrap();
    std::fs::write(source.path().join("outside.txt"), "Outside boundary\n").unwrap();
    std::os::unix::fs::symlink(&root, source.path().join("logical")).unwrap();
    std::os::unix::fs::symlink(source.path(), root.join("escape")).unwrap();
    let snapshot = SnapshotService::new(storage.path().join("snapshots"))
        .capture(
            ReviewTarget::Directory {
                path: source.path().join("logical").to_str().unwrap().into(),
            },
            ReviewComparison::Directory,
        )
        .await
        .unwrap();
    assert_eq!(
        command(
            Path::new(&snapshot.root),
            &["ls-tree", "-r", "--name-only", &snapshot.head_commit]
        ),
        "PLAN.md"
    );
}
