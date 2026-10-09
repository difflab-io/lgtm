//! Negative paths. Assert that the Connect error envelope carries the
//! right code / message instead of a generic 500.

use lgtm_e2e_common::TestEnv;

#[test]
fn create_thread_rejects_empty_body() {
    let env = TestEnv::spawn();
    let repo = env.seed_repo();
    let alice = env.upsert_user("alice", lgtm_e2e_common::AuthorKind::User);

    let err = env
        .try_create_thread(&repo, "HEAD", "src/lib.rs", 1, alice.id, "")
        .expect_err("empty body should be rejected");

    let msg = format!("{err:#}");
    assert!(msg.contains("400"), "expected 400 status, got: {msg}");
    assert!(msg.contains("body"), "expected message to mention body, got: {msg}");
}

#[test]
fn create_thread_rejects_empty_file_path() {
    let env = TestEnv::spawn();
    let repo = env.seed_repo();
    let alice = env.upsert_user("alice", lgtm_e2e_common::AuthorKind::User);

    let err = env
        .try_create_thread(&repo, "HEAD", "", 1, alice.id, "nit")
        .expect_err("empty file_path should be rejected");

    assert!(format!("{err:#}").contains("file_path"));
}
