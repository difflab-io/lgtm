//! E2E coverage for `lgtm` CLI subcommands against a live `lgtm-server`.
//!
//! Each test spins up its own `TestEnv` (fresh db + port) so they can run in
//! parallel without stepping on one another. The CLI binary is located via
//! `lgtm_cli_binary()` which mirrors `lgtm_server_binary()` — both read from
//! `target/<profile>/`.

use std::process::Command;

use lgtm_e2e_common::{AuthorKind, TestEnv, env::lgtm_cli_binary};

/// Build a CLI invocation already pointed at the test server + repo. Hides
/// the --backend/--as plumbing so test bodies read like shell transcripts.
fn cli(env: &TestEnv) -> CliRun {
    CliRun {
        backend: env.base_url().to_owned(),
        author: None,
        kind: None,
    }
}

struct CliRun {
    backend: String,
    author: Option<String>,
    kind: Option<String>,
}

impl CliRun {
    fn as_author(mut self, name: &str, kind: AuthorKind) -> Self {
        self.author = Some(name.into());
        self.kind = Some(
            match kind {
                AuthorKind::User => "user",
                AuthorKind::Agent => "agent",
                AuthorKind::Unspecified => "agent",
            }
            .into(),
        );
        self
    }

    fn run(&self, args: &[&str]) -> std::process::Output {
        let bin = lgtm_cli_binary();
        if !bin.exists() {
            panic!(
                "lgtm binary not found at {}. Run `cargo build -p lgtm-cli` first.",
                bin.display()
            );
        }
        let mut cmd = Command::new(bin);
        cmd.arg("--backend").arg(&self.backend);
        if let Some(a) = &self.author {
            cmd.arg("--as").arg(a);
        }
        if let Some(k) = &self.kind {
            cmd.arg("--as-kind").arg(k);
        }
        for a in args {
            cmd.arg(a);
        }
        cmd.output().expect("spawn lgtm cli")
    }
}

fn stdout(out: &std::process::Output) -> String {
    assert!(
        out.status.success(),
        "lgtm cli failed: {}\nstderr: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout.clone()).expect("utf-8 stdout")
}

#[test]
fn files_lists_changed_paths() {
    let env = TestEnv::spawn();
    let repo = env.seed_repo();

    let out = cli(&env).run(&["files", repo.path().to_str().unwrap(), "--base", "HEAD"]);
    let text = stdout(&out);

    assert!(text.contains("modified   src/lib.rs"), "got: {text}");
    assert!(text.contains("deleted    src/util.rs"), "got: {text}");
    assert!(text.contains("untracked  src/added.rs"), "got: {text}");
}

#[test]
fn diff_prints_unified_patch_for_file() {
    let env = TestEnv::spawn();
    let repo = env.seed_repo();

    let out = cli(&env).run(&[
        "diff",
        repo.path().to_str().unwrap(),
        "--path",
        "src/lib.rs",
        "--base",
        "HEAD",
    ]);
    let text = stdout(&out);

    assert!(text.contains("+++ b/src/lib.rs"), "diff header missing: {text}");
    assert!(text.contains("+pub fn hi() {}"), "added line missing: {text}");
}

#[test]
fn user_upsert_prints_id() {
    let env = TestEnv::spawn();
    let out = cli(&env).run(&["user", "upsert", "claude", "--kind", "agent"]);
    let id: i64 = stdout(&out).trim().parse().expect("id is an integer");
    assert!(id > 0);
}

#[test]
fn comment_add_reply_resolve_round_trip() {
    let env = TestEnv::spawn();
    let repo = env.seed_repo();

    let tid = stdout(
        &cli(&env)
            .as_author("claude", AuthorKind::Agent)
            .run(&[
                "comment",
                "add",
                repo.path().to_str().unwrap(),
                "--path",
                "src/lib.rs",
                "--line",
                "2",
                "--base",
                "HEAD",
                "--body",
                "nit",
            ]),
    )
    .trim()
    .parse::<i64>()
    .expect("thread id is int");

    stdout(
        &cli(&env)
            .as_author("alice", AuthorKind::User)
            .run(&["comment", "reply", "--thread", &tid.to_string(), "--body", "ack"]),
    );

    let listed = stdout(&cli(&env).run(&[
        "threads",
        repo.path().to_str().unwrap(),
        "--base",
        "HEAD",
    ]));

    assert!(listed.contains("[open]"), "thread should still be open: {listed}");
    assert!(listed.contains("claude: nit"), "agent comment missing: {listed}");
    assert!(listed.contains("alice: ack"), "user reply missing: {listed}");

    stdout(&cli(&env).run(&["comment", "resolve", "--thread", &tid.to_string()]));

    let unresolved = stdout(&cli(&env).run(&[
        "threads",
        repo.path().to_str().unwrap(),
        "--base",
        "HEAD",
        "--unresolved",
    ]));
    assert!(unresolved.trim().is_empty(), "no open threads expected: {unresolved}");
}

#[test]
fn threads_json_is_valid() {
    let env = TestEnv::spawn();
    let repo = env.seed_repo();
    let alice = env.upsert_user("alice", AuthorKind::User);
    env.create_thread(&repo, "HEAD", "src/lib.rs", 1, alice.id, "nit");

    let json = stdout(&cli(&env).run(&[
        "threads",
        repo.path().to_str().unwrap(),
        "--base",
        "HEAD",
        "--json",
    ]));
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid json");
    let threads = parsed["threads"].as_array().expect("threads array");
    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0]["comments"][0]["body"], "nit");
}
