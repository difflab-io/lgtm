//! User workflows through the built CLI and actual persisted backend.
mod common;
use common::Fixture;

// tests -----------------------------------------------------------------------
#[test]
fn should_review_ignored_directory_export_and_resume_after_restart() {
    let mut f = Fixture::start();
    assert!(
        std::process::Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(f.files.path())
            .status()
            .unwrap()
            .success()
    );
    std::fs::write(f.files.path().join(".gitignore"), ".difflab/\n").unwrap();
    let plans = f.files.path().join(".difflab/plans/private-plan");
    std::fs::create_dir_all(&plans).unwrap();
    std::fs::write(plans.join("PLAN.md"), "# Plan\nFirst task\n").unwrap();
    let path = plans.to_str().unwrap();
    let start = f.json(&["session", "start", "--notes", path]);
    let id = start["session"]["id"].as_str().unwrap();
    let comment = f.json(&[
        "comments",
        "add",
        "--notes",
        path,
        "--file",
        "PLAN.md",
        "--line",
        "2",
        "--body",
        "Clarify the task",
    ]);
    let comment_id = comment["comment"]["id"].as_str().unwrap();
    f.json(&[
        "comments",
        "edit",
        comment_id,
        "--notes",
        path,
        "--body",
        "Clarify the acceptance test",
    ]);
    let result = f.json(&["comments", "list", "--notes", path]);
    assert!(result.to_string().contains("Clarify the acceptance test"));
    let export = f.json(&["session", "export", "--notes", path]);
    assert!(
        export["markdown"]
            .as_str()
            .unwrap()
            .contains("Clarify the acceptance test")
    );
    let target = f.files.path().join("review-target");
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("PLAN.md"), "# Target review\n").unwrap();
    let target_path = target.to_str().unwrap();
    let target_session = f.json(&["session", "start", "--notes", target_path]);
    let target_id = target_session["session"]["id"].as_str().unwrap();
    let promotion = f.json(&[
        "comments",
        "promote",
        "--notes",
        path,
        "--target-session",
        target_id,
        "--comment",
        comment_id,
        "--file",
        "PLAN.md",
    ]);
    assert_eq!(
        promotion["comments"][0]["body"],
        "Clarify the acceptance test"
    );
    let promoted_export = f.json(&["session", "export", "--notes", target_path]);
    assert!(
        promoted_export["markdown"]
            .as_str()
            .unwrap()
            .contains(comment_id)
    );
    f.restart();
    let resumed = f.json(&["session", "start", "--notes", path]);
    assert_eq!(resumed["session"]["id"], id);
    let closed = f.json(&["session", "complete", "--notes", path]);
    assert_eq!(closed["session"]["state"], "completed");
    let next = f.json(&["session", "start", "--notes", path]);
    assert_ne!(next["session"]["id"], id);
    let list = f.json(&["comments", "list", "--notes", path]);
    assert!(list.to_string().contains("Clarify the acceptance test"));
}

#[test]
fn should_return_json_error_without_starting_backend() {
    let home = tempfile::tempdir().unwrap();
    let result = std::process::Command::new(common::binary("lgtm"))
        .args(["sessions", "list", "--json"])
        .env("LGTM_HOME", home.path())
        .env_remove("LGTM_RPC_URL")
        .output()
        .unwrap();
    assert!(!result.status.success());
    let value: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert!(
        value["error"]["message"]
            .as_str()
            .unwrap()
            .contains("backend")
    );
}
