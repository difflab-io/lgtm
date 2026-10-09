//! E2E coverage for comment threads: create → reply → resolve → list filters.

use lgtm_e2e_common::{AuthorKind, TestEnv};

#[test]
fn upsert_user_is_idempotent_on_name_and_kind() {
    let env = TestEnv::spawn();

    let first = env.upsert_user("alice", AuthorKind::User);
    let second = env.upsert_user("alice", AuthorKind::User);
    let distinct = env.upsert_user("alice", AuthorKind::Agent);

    assert_eq!(first.id, second.id, "same (name,kind) ⇒ same id");
    assert_ne!(first.id, distinct.id, "different kind ⇒ distinct user");
}

#[test]
fn thread_lifecycle_create_reply_resolve_list() {
    let env = TestEnv::spawn();
    let repo = env.seed_repo();
    let alice = env.upsert_user("alice", AuthorKind::User);
    let claude = env.upsert_user("claude", AuthorKind::Agent);

    let thread = env.create_thread(&repo, "HEAD", "src/lib.rs", 2, alice.id, "nit");
    assert_eq!(thread.comments.len(), 1);
    assert_eq!(thread.comments[0].body, "nit");
    assert!(!thread.resolved);

    env.append_comment(thread.id, claude.id, "ack");

    let listed = env.list_threads(&repo, "HEAD");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].comments.len(), 2);
    assert_eq!(listed[0].comments[1].author.name, "claude");
    assert_eq!(listed[0].comments[1].author.kind, AuthorKind::Agent);

    let resolved = env.resolve_thread(thread.id);
    assert!(resolved.resolved);

    let open_only = env.list_threads_full(&repo, "HEAD", None, true);
    assert!(open_only.is_empty(), "resolved thread should be filtered out");
}

#[test]
fn threads_are_scoped_by_file() {
    let env = TestEnv::spawn();
    let repo = env.seed_repo();
    let alice = env.upsert_user("alice", AuthorKind::User);

    env.create_thread(&repo, "HEAD", "src/lib.rs", 1, alice.id, "lib thread");
    env.create_thread(&repo, "HEAD", "src/util.rs", 1, alice.id, "util thread");

    let for_lib = env.list_threads_full(&repo, "HEAD", Some("src/lib.rs"), false);
    assert_eq!(for_lib.len(), 1);
    assert_eq!(for_lib[0].comments[0].body, "lib thread");

    let for_util = env.list_threads_full(&repo, "HEAD", Some("src/util.rs"), false);
    assert_eq!(for_util.len(), 1);
    assert_eq!(for_util[0].comments[0].body, "util thread");
}
