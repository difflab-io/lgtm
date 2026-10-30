use lgtm_backend::models::*;

// tests -----------------------------------------------------------------------
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
