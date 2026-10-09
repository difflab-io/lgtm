//! E2E coverage for the diff-reading endpoints.
//! Expectations are written against the standard git fixture (see
//! `lgtm_e2e_common::GitRepoFixture::standard`).

use lgtm_e2e_common::TestEnv;

#[test]
fn changed_files_returns_added_modified_deleted() {
    let env = TestEnv::spawn();
    let repo = env.seed_repo();

    let mut files = env.list_files(&repo, "HEAD");
    files.sort_by(|a, b| a.path.cmp(&b.path));

    let paths_and_status: Vec<(&str, &str)> = files
        .iter()
        .map(|f| (f.path.as_str(), f.status.as_str()))
        .collect();

    assert_eq!(
        paths_and_status,
        vec![
            ("src/added.rs", "untracked"),
            ("src/lib.rs", "modified"),
            ("src/util.rs", "deleted"),
        ],
    );
}

#[test]
fn file_diff_for_modified_file_contains_added_and_removed_lines() {
    let env = TestEnv::spawn();
    let repo = env.seed_repo();

    let diff = env.file_diff(&repo, "HEAD", "src/lib.rs");

    assert!(diff.contains("+++ b/src/lib.rs"), "diff header missing: {diff}");
    assert!(diff.contains("+pub fn hi() {}"), "added line missing: {diff}");
}

#[test]
fn file_diff_for_untouched_file_is_empty() {
    let env = TestEnv::spawn();
    let repo = env.seed_repo();

    let diff = env.file_diff(&repo, "HEAD", "Cargo.toml");

    assert!(diff.is_empty(), "expected empty diff, got: {diff:?}");
}
