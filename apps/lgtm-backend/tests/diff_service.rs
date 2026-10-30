//! Read-only content workflows, independent of review setup or database records.
use lgtm_backend::{models::*, services::diff::DiffService};
use std::path::Path;

// fixtures --------------------------------------------------------------------
fn git(root: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}

fn commit(root: &Path, message: &str) -> String {
    git(
        root,
        &[
            "-c",
            "user.name=E2E",
            "-c",
            "user.email=e2e@example.test",
            "commit",
            "-qm",
            message,
        ],
    );
    git(root, &["rev-parse", "HEAD"])
}

// tests -----------------------------------------------------------------------
#[tokio::test]
async fn should_list_ignored_directory_metadata_and_read_only_the_selected_text_file() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    std::fs::create_dir_all(root.join(".difflab/plans/specific-plan")).unwrap();
    let relative = ".difflab/plans/specific-plan/PLAN.md";
    let bytes = b"# Plan\r\nAcceptance criteria";
    std::fs::write(root.join(relative), bytes).unwrap();
    std::fs::write(root.join("unrelated.bin"), [0, 255, 0]).unwrap();
    let service = DiffService::new();
    let diff = service
        .get_diff(
            ReviewTarget::Directory {
                path: root.to_str().unwrap().into(),
            },
            None,
        )
        .await
        .unwrap();
    let files = service.get_files(&diff).await.unwrap();
    assert_eq!(
        files.len(),
        2,
        "metadata listing does not parse unrelated binary bodies"
    );
    assert!(files.iter().any(|file| file.path == relative));
    let selected = service.get_file(&diff, relative).await.unwrap();
    let new = selected.new.unwrap();
    assert!(selected.old.is_none());
    assert_eq!(new.content.as_bytes(), bytes);
    assert_eq!(new.source.content_hash(), AnchorSource::hash(bytes));
    assert!(matches!(new.source, AnchorSource::LocalFile { .. }));
    assert!(service.get_file(&diff, "../outside").await.is_err());
    std::fs::write(root.join(relative), "new saved bytes\n").unwrap();
    assert!(
        service.get_file(&diff, relative).await.is_err(),
        "stale metadata cannot be presented as the observed source"
    );
}

#[tokio::test]
async fn should_distinguish_index_working_and_committed_sides_without_changing_the_checkout() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    git(root, &["init", "-q"]);
    std::fs::write(root.join("file.txt"), b"base\n").unwrap();
    git(root, &["add", "file.txt"]);
    let base = commit(root, "base");
    std::fs::write(root.join("file.txt"), b"staged\r\nselected").unwrap();
    git(root, &["add", "file.txt"]);
    let oid = git(root, &["rev-parse", ":file.txt"]);
    std::fs::write(root.join("file.txt"), b"working\nselected\n").unwrap();
    let before = git(root, &["status", "--porcelain"]);
    let service = DiffService::new();
    let target = ReviewTarget::LocalGit {
        path: root.to_str().unwrap().into(),
        branch: None,
    };
    let staged = service
        .get_diff(
            target.clone(),
            Some(ReviewComparison::Staged {
                base_commit: base.clone(),
            }),
        )
        .await
        .unwrap();
    let content = service.get_file(&staged, "file.txt").await.unwrap();
    assert_eq!(content.old.unwrap().content, "base\n");
    let new = content.new.unwrap();
    assert_eq!(new.content.as_bytes(), b"staged\r\nselected");
    assert!(matches!(new.source, AnchorSource::IndexFile { blob_oid, .. } if blob_oid == oid));
    let working = service
        .get_diff(
            target.clone(),
            Some(ReviewComparison::Working {
                base_commit: base.clone(),
            }),
        )
        .await
        .unwrap();
    let new = service
        .get_file(&working, "file.txt")
        .await
        .unwrap()
        .new
        .unwrap();
    assert!(matches!(new.source, AnchorSource::WorkingFile { .. }));
    assert_eq!(new.content, "working\nselected\n");
    assert_eq!(git(root, &["status", "--porcelain"]), before);
    let head = commit(root, "commit index only");
    let branch = service
        .get_diff(
            target,
            Some(ReviewComparison::Branch {
                base_commit: base,
                head_commit: head.clone(),
            }),
        )
        .await
        .unwrap();
    let new = service
        .get_file(&branch, "file.txt")
        .await
        .unwrap()
        .new
        .unwrap();
    assert!(matches!(new.source, AnchorSource::Commit { sha, .. } if sha == head));
    assert_eq!(new.content.as_bytes(), b"staged\r\nselected");
    assert_eq!(
        std::fs::read(root.join("file.txt")).unwrap(),
        b"working\nselected\n"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn should_skip_nested_symlinks_and_allow_an_explicit_root_symlink() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("physical");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("plan.md"), "plan\n").unwrap();
    std::os::unix::fs::symlink(&root, temp.path().join("selected-root")).unwrap();
    std::os::unix::fs::symlink(temp.path(), root.join("escape")).unwrap();
    let service = DiffService::new();
    let diff = service
        .get_diff(
            ReviewTarget::Directory {
                path: temp.path().join("selected-root").to_str().unwrap().into(),
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(service.get_files(&diff).await.unwrap().len(), 1);
    assert!(
        service
            .get_file(&diff, "escape/physical/plan.md")
            .await
            .is_err()
    );
}

#[tokio::test]
async fn should_read_native_anchor_sources_without_reimplementing_saved_bytes_in_clients() {
    use lgtm_backend::database::types::SourceKind;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let service = DiffService::new();
    let location = root.to_str().unwrap();
    assert!(
        service
            .get_source(location, "missing", SourceKind::LocalFile, None)
            .await
            .unwrap()
            .is_none()
    );
    std::fs::write(root.join("empty"), "").unwrap();
    let empty = service
        .get_source(location, "empty", SourceKind::LocalFile, None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(empty.content, "");
    assert_eq!(empty.source.content_hash(), AnchorSource::hash(b""));
    assert!(
        service
            .get_source(location, "../outside", SourceKind::LocalFile, None)
            .await
            .is_err()
    );
    std::fs::write(root.join("binary"), [0, 255]).unwrap();
    assert!(
        service
            .get_source(location, "binary", SourceKind::LocalFile, None)
            .await
            .is_err()
    );
    std::fs::write(root.join("invalid-utf8"), [255, 254]).unwrap();
    assert!(
        service
            .get_source(location, "invalid-utf8", SourceKind::LocalFile, None)
            .await
            .is_err()
    );

    git(&root, &["init", "-q"]);
    let path = "space [literal] ü.txt";
    let original = "before\r\nselected ü\r\nafter";
    std::fs::write(root.join(path), original).unwrap();
    git(&root, &["add", "--", path]);
    let head = commit(&root, "source fixture");
    let oid = git(&root, &["rev-parse", &format!(":{path}")]);
    let staged = service
        .get_source(location, path, SourceKind::IndexFile, None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(staged.content, original);
    assert!(matches!(&staged.source, AnchorSource::IndexFile { blob_oid, .. } if blob_oid == &oid));
    let preserved = service
        .get_source(location, path, SourceKind::WorkingFile, None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        preserved.source, staged.source,
        "working bytes equal to the index use preservable index evidence"
    );
    std::fs::write(root.join(path), "unstaged source\n").unwrap();
    let working = service
        .get_source(location, path, SourceKind::WorkingFile, None)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(working.source, AnchorSource::WorkingFile { .. }));
    assert_ne!(working.source.content_hash(), staged.source.content_hash());
    let committed = service
        .get_source(location, path, SourceKind::Commit, Some(&head))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(committed.content, original);
    assert!(matches!(committed.source, AnchorSource::Commit { .. }));
    assert!(
        service
            .get_source(location, "missing", SourceKind::IndexFile, None)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        service
            .get_source(location, "missing", SourceKind::Commit, Some(&head))
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        service
            .get_source(location, path, SourceKind::IndexFile, Some(&head))
            .await
            .is_err()
    );
    assert_eq!(git(&root, &["rev-parse", "HEAD"]), head);
    assert_eq!(git(&root, &["rev-parse", &format!(":{path}")]), oid);
}
