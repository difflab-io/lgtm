use serde_json::Value;
use std::{
    path::Path,
    process::{Command, Output},
};
fn run(cwd: &Path, home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lgtm"))
        .env("LGTM_HOME", home)
        .env("GIT_CONFIG_GLOBAL", home.join("missing-git-config"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env_remove("LGTM_DATABASE_URL")
        .env_remove("LGTM_FORGE_FIXTURE")
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap()
}
fn ok(cwd: &Path, home: &Path, args: &[&str]) -> Value {
    let out = run(cwd, home, args);
    assert!(
        out.status.success(),
        "{:?}: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
fn git(path: &Path, args: &[&str]) {
    assert!(
        Command::new("git")
            .arg("-C")
            .arg(path)
            .args(args)
            .status()
            .unwrap()
            .success()
    );
}
#[test]
fn notes_round_trip_across_processes_preserves_context_and_actors() {
    let cwd = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let c = ok(
        cwd.path(),
        home.path(),
        &[
            "comments",
            "add",
            "--notes",
            "--as",
            "Alice <alice@example.com>",
            "--body",
            "Root **Markdown**",
            "--file",
            "plan.md",
        ],
    );
    let thread = c["thread_id"].as_str().unwrap();
    let first = c["origin_session_id"].as_str().unwrap();
    let reply = ok(
        cwd.path(),
        home.path(),
        &[
            "comments",
            "reply",
            thread,
            "--notes",
            "--as",
            "Agent <agent@example.com>",
            "--model",
            "model-name",
            "--body",
            "Reply",
        ],
    );
    assert_ne!(c["author"]["id"], reply["author"]["id"]);
    let closed = ok(
        cwd.path(),
        home.path(),
        &[
            "session",
            "complete",
            "--notes",
            "--as",
            "Alice <alice@example.com>",
            "--request-key",
            "one",
        ],
    );
    assert_eq!(closed["id"], first);
    assert_eq!(closed["outcome"], "saved");
    let next = ok(
        cwd.path(),
        home.path(),
        &[
            "session",
            "start",
            "--notes",
            "--as",
            "Alice <alice@example.com>",
        ],
    );
    assert_ne!(next["id"], first);
    let same = ok(
        cwd.path(),
        home.path(),
        &[
            "session",
            "complete",
            "--notes",
            "--as",
            "Alice <alice@example.com>",
            "--request-key",
            "one",
        ],
    );
    assert_eq!(same["id"], first);
    ok(
        cwd.path(),
        home.path(),
        &[
            "comments",
            "resolve",
            thread,
            "--notes",
            "--as",
            "Alice <alice@example.com>",
        ],
    );
    let reopened = ok(
        cwd.path(),
        home.path(),
        &[
            "comments",
            "reopen",
            thread,
            "--notes",
            "--as",
            "Alice <alice@example.com>",
        ],
    );
    assert!(reopened["resolved_by"].is_null());
    let listed = ok(cwd.path(), home.path(), &["comments", "list", "--notes"]);
    assert_eq!(listed["threads"].as_array().unwrap().len(), 1);
    assert_eq!(listed["comments"].as_array().unwrap().len(), 2);
    let exported = run(
        cwd.path(),
        home.path(),
        &["session", "export", "--notes", "--session", first],
    );
    assert!(exported.status.success());
    let text = String::from_utf8(exported.stdout).unwrap();
    assert!(
        text.contains("Root **Markdown**")
            && text.contains("model-name")
            && text.contains("CURRENT")
    );
    let output = cwd.path().join("review.md");
    let exported = run(
        cwd.path(),
        home.path(),
        &[
            "session",
            "export",
            "--notes",
            "--output",
            output.to_str().unwrap(),
        ],
    );
    assert!(exported.status.success() && exported.stdout.is_empty() && output.exists());
    assert!(
        !run(
            cwd.path(),
            home.path(),
            &[
                "session",
                "start",
                "--notes",
                "--pr",
                "acme/repo#1",
                "--as",
                "Alice <alice@example.com>"
            ]
        )
        .status
        .success()
    );
    assert!(
        !run(
            cwd.path(),
            home.path(),
            &[
                "comments",
                "reply",
                "missing",
                "--notes",
                "--as",
                "Alice <alice@example.com>",
                "--body",
                "bad"
            ]
        )
        .status
        .success()
    );
    assert!(
        !run(
            cwd.path(),
            home.path(),
            &["comments", "add", "--notes", "--body", "Missing identity"]
        )
        .status
        .success()
    );
}
#[test]
fn inferred_branch_line_anchors_filters_detached_head_and_read_only_status() {
    let repo = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    git(repo.path(), &["init", "-b", "feature/test"]);
    git(repo.path(), &["config", "user.name", "Alice"]);
    git(repo.path(), &["config", "user.email", "alice@example.com"]);
    std::fs::write(repo.path().join("file.rs"), "line one\nline two\n").unwrap();
    git(repo.path(), &["add", "file.rs"]);
    git(repo.path(), &["commit", "-m", "fixture"]);
    let empty = home.path().join("unused-home");
    let status = run(repo.path(), &empty, &["status", "--json"]);
    assert!(status.status.success());
    assert!(!empty.exists());
    let c = ok(
        repo.path(),
        home.path(),
        &[
            "comments",
            "add",
            "--body",
            "Line anchor",
            "--file",
            "file.rs",
            "--line",
            "1",
            "--end-line",
            "2",
            "--side",
            "new",
        ],
    );
    assert_eq!(c["position"], 0);
    let list = ok(repo.path(), home.path(), &["sessions", "list"]);
    assert_eq!(list.as_array().unwrap().len(), 1);
    let all = ok(repo.path(), home.path(), &["sessions", "list", "--all"]);
    assert_eq!(all.as_array().unwrap().len(), 1);
    assert!(
        !run(
            repo.path(),
            home.path(),
            &[
                "comments", "add", "--body", "bad", "--file", "file.rs", "--line", "0", "--side",
                "new"
            ]
        )
        .status
        .success()
    );
    let before = Command::new("git")
        .arg("-C")
        .arg(repo.path())
        .args(["status", "--porcelain"])
        .output()
        .unwrap();
    assert!(before.stdout.is_empty());
    git(repo.path(), &["checkout", "--detach"]);
    assert!(
        run(repo.path(), home.path(), &["sessions", "list"])
            .status
            .success()
    );
    assert!(
        !run(repo.path(), home.path(), &["session", "start"])
            .status
            .success()
    );
    ok(
        repo.path(),
        home.path(),
        &[
            "session",
            "start",
            "--notes",
            "--as",
            "Alice <alice@example.com>",
        ],
    );
}
#[test]
fn pr_requires_explicit_fixture_and_stub_submission_never_claims_remote_success() {
    let cwd = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    assert!(
        !run(
            cwd.path(),
            home.path(),
            &[
                "session",
                "start",
                "--pr",
                "acme/repo#42",
                "--as",
                "Alice <alice@example.com>"
            ]
        )
        .status
        .success()
    );
    let fixture = home.path().join("forge.json");
    std::fs::write(
        &fixture,
        r#"{"complete":true,"head":"abc","base":"base","threads":[]}"#,
    )
    .unwrap();
    let call = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_lgtm"))
            .env("LGTM_HOME", home.path())
            .env("LGTM_FORGE_FIXTURE", &fixture)
            .current_dir(cwd.path())
            .args(args)
            .output()
            .unwrap()
    };
    let start = call(&[
        "session",
        "start",
        "--pr",
        "acme/repo#42",
        "--as",
        "Alice <alice@example.com>",
    ]);
    assert!(
        start.status.success(),
        "{}",
        String::from_utf8_lossy(&start.stderr)
    );
    assert!(String::from_utf8_lossy(&start.stderr).contains("STUB"));
    let anchored = call(&[
        "comments",
        "add",
        "--pr",
        "acme/repo#42",
        "--as",
        "Alice <alice@example.com>",
        "--body",
        "Old side",
        "--file",
        "file.rs",
        "--line",
        "1",
        "--side",
        "old",
    ]);
    assert!(
        anchored.status.success(),
        "{}",
        String::from_utf8_lossy(&anchored.stderr)
    );
    let submit = call(&[
        "session",
        "submit",
        "--pr",
        "acme/repo#42",
        "--as",
        "Alice <alice@example.com>",
        "--outcome",
        "approve",
    ]);
    assert!(submit.status.success());
    let s: Value = serde_json::from_slice(&submit.stdout).unwrap();
    assert_eq!(s["submission_status"], "unsubmitted");
    assert!(s["forge_review_id"].is_null());
}

#[cfg(feature = "postgres")]
#[tokio::test]
async fn cli_uses_postgres_without_command_changes() {
    let base = std::env::var("LGTM_TEST_POSTGRES_URL")
        .expect("postgres CLI tests require a disposable LGTM_TEST_POSTGRES_URL");
    use sea_orm::ConnectionTrait;
    let pool = sea_orm::Database::connect(&base).await.unwrap();
    let schema = format!("cli_{}", uuid::Uuid::new_v4().simple());
    pool.execute_unprepared(&format!("CREATE SCHEMA {schema}"))
        .await
        .unwrap();
    pool.close().await.unwrap();
    let url = format!(
        "{base}{}options=-c%20search_path%3D{schema}",
        if base.contains('?') { '&' } else { '?' }
    );
    let cwd = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let call = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_lgtm"))
            .env("LGTM_HOME", home.path())
            .env("LGTM_DATABASE_URL", &url)
            .current_dir(cwd.path())
            .args(args)
            .output()
            .unwrap()
    };
    for args in [
        vec![
            "comments",
            "add",
            "--notes",
            "--as",
            "Alice <alice@example.com>",
            "--body",
            "Postgres persists",
        ],
        vec![
            "session",
            "complete",
            "--notes",
            "--as",
            "Alice <alice@example.com>",
        ],
    ] {
        let out = call(&args);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let export = call(&["session", "export", "--notes"]);
    assert!(export.status.success());
    assert!(String::from_utf8_lossy(&export.stdout).contains("Postgres persists"));
}
