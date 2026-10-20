//! User workflows through the built CLI and actual persisted backend.
mod common;
use common::Fixture;

// fixtures --------------------------------------------------------------------
fn git(f: &Fixture, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(["-c", "user.name=E2E", "-c", "user.email=e2e@example.test"])
        .args(args)
        .current_dir(f.files.path())
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

// tests -----------------------------------------------------------------------
#[test]
fn should_export_numbered_markdown_with_old_side_replies_and_final_feedback_without_mutation() {
    let f = Fixture::start();
    git(&f, &["init", "-q", "-b", "main"]);
    let file = "review.md";
    std::fs::write(f.files.path().join(file), "before\nold selection\nafter\n").unwrap();
    git(&f, &["add", "."]);
    git(&f, &["commit", "-qm", "base"]);
    std::fs::write(f.files.path().join(file), "before\nnew selection\nafter\n").unwrap();
    f.json(&["session", "start", "--mode", "working"]);
    let body = "Explain the fallback.\n\n- Keep **Markdown** and `code`.";
    let root = f.json(&[
        "comments", "add", "--mode", "working", "--file", file, "--line", "2", "--side", "old",
        "--body", body,
    ]);
    f.json(&[
        "comments",
        "reply",
        root["comment"]["threadId"].as_str().unwrap(),
        "--mode",
        "working",
        "--body",
        "Use the documented alternative.",
    ]);
    f.json(&[
        "session",
        "complete",
        "--mode",
        "working",
        "--body",
        "Please address these cases before merging.",
    ]);
    let before = f.json(&["comments", "list", "--mode", "working"]);
    let output = f.files.path().join("feedback.md");
    let exported = f.json(&[
        "session",
        "export",
        "--mode",
        "working",
        "--output",
        output.to_str().unwrap(),
    ]);
    let markdown = exported["markdown"].as_str().unwrap();
    assert_eq!(std::fs::read_to_string(output).unwrap(), markdown);
    for expected in [
        "1. **[COMMENT]**",
        "`review.md:~2`",
        "old (left) side",
        "Explain the fallback.",
        "- Keep **Markdown** and `code`.",
        "**Reply",
        "Use the documented alternative.",
        "## Overall feedback",
        "Please address these cases before merging.",
    ] {
        assert!(
            markdown.contains(expected),
            "Missing {expected:?} in {markdown}"
        );
    }
    for comment in before["comments"].as_array().unwrap() {
        assert!(!markdown.contains(comment["id"].as_str().unwrap()));
    }
    for metadata in ["sha256:", "originSessionId", "postingStatus", "contextId"] {
        assert!(
            !markdown.contains(metadata),
            "internal metadata: {metadata}"
        );
    }
    assert_eq!(
        f.json(&["comments", "list", "--mode", "working"]),
        before,
        "export is a read-only operation"
    );
}

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
    let id = start["review"]["id"].as_str().unwrap();
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
    let target_id = target_session["review"]["id"].as_str().unwrap();
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
    let markdown = promoted_export["markdown"].as_str().unwrap();
    assert!(markdown.contains("1. **[COMMENT]**"));
    assert!(markdown.contains("`PLAN.md`"));
    assert!(markdown.contains("Clarify the acceptance test"));
    assert!(
        !markdown.contains(comment_id),
        "exports omit internal provenance IDs"
    );
    let source_after_export = f.json(&["comments", "list", "--notes", path]);
    assert!(
        source_after_export["comments"]
            .as_array()
            .unwrap()
            .iter()
            .any(|comment| comment["id"] == comment_id
                && comment["body"] == "Clarify the acceptance test"),
        "promotion and export retain source feedback"
    );
    f.restart();
    let resumed = f.json(&["session", "start", "--notes", path]);
    assert_eq!(resumed["review"]["id"], id);
    let closed = f.json(&["session", "complete", "--notes", path]);
    assert_eq!(closed["review"]["state"], "completed");
    let next = f.json(&["session", "start", "--notes", path]);
    assert_ne!(next["review"]["id"], id);
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

#[test]
fn should_add_file_notes_without_a_visible_review_and_replay_exact_guarded_mutations() {
    let f = Fixture::start();
    let file = f.files.path().join("notes ü.md");
    std::fs::write(&file, b"before\r\nselected\r\nafter").unwrap();
    let path = file.to_str().unwrap();
    let request = [
        "comments",
        "add",
        "--notes",
        path,
        "--file",
        "notes ü.md",
        "--line",
        "2",
        "--body",
        "Explain the fallback",
        "--expected-version",
        "0",
        "--request-key",
        "note-once",
    ];
    let saved = f.json(&request);
    let retried = f.json(&request);
    assert_eq!(saved["comment"]["id"], retried["comment"]["id"]);
    let notes = f.json(&["comments", "list", "--notes", path]);
    assert_eq!(notes["comments"].as_array().unwrap().len(), 1);
    assert_eq!(
        notes["threads"][0]["anchor"]["evidence"]["selectedText"],
        "selected"
    );
    assert!(
        notes["threads"][0]["anchor"]["source"]["localFile"]["contentHash"]
            .as_str()
            .unwrap()
            .starts_with("sha256:")
    );
    let reviews = f.json(&["sessions", "list", "--all"]);
    assert!(
        reviews["reviews"].as_array().unwrap().is_empty(),
        "ordinary file notes never create a visible review"
    );
    let markdown = f.json(&["session", "export", "--notes", path]);
    assert!(
        markdown["markdown"]
            .as_str()
            .unwrap()
            .contains("1. **[NOTE]**")
    );
    let id = notes["comments"][0]["id"].as_str().unwrap();
    let version = notes["comments"][0]["version"].as_str().unwrap();
    std::fs::remove_file(&file).unwrap();
    let deletion = [
        "comments",
        "delete",
        id,
        "--notes",
        path,
        "--expected-version",
        version,
        "--request-key",
        "delete-once",
    ];
    let deleted = f.json(&deletion);
    assert_eq!(
        f.json(&deletion),
        deleted,
        "deletion replay survives missing file and object"
    );
    let after = f.json(&["comments", "list", "--notes", path]);
    assert!(after["comments"].as_array().unwrap().is_empty());
}

#[test]
fn should_preserve_staged_source_and_discussion_when_committing_and_selecting_a_branch_review() {
    let f = Fixture::start();

    git(&f, &["init", "-q", "-b", "main"]);
    let file = "source [literal] ü.txt";
    std::fs::write(f.files.path().join(file), "before\noriginal\nafter\n").unwrap();
    git(&f, &["add", "--", file]);
    git(&f, &["commit", "-qm", "base"]);
    let base = git(&f, &["rev-parse", "HEAD"]);
    git(&f, &["checkout", "-qb", "feature"]);
    std::fs::write(f.files.path().join(file), "before\r\nselected\r\nafter").unwrap();
    git(&f, &["add", "--", file]);
    let blob = git(&f, &["rev-parse", &format!(":{file}")]);
    std::fs::write(
        f.files.path().join(file),
        "before\nunsaved to index\nafter\n",
    )
    .unwrap();
    f.json(&["session", "start", "--mode", "staged"]);
    let saved = f.json(&[
        "comments",
        "add",
        "--file",
        file,
        "--line",
        "2",
        "--body",
        "Keep this staged behavior",
    ]);
    let original = f.json(&["comments", "list"]);
    let anchor = original["threads"][0]["anchor"].clone();
    assert_eq!(anchor["source"]["indexFile"]["blobOid"], blob);
    assert_eq!(anchor["evidence"]["selectedText"], "selected");
    assert!(
        anchor["source"]["commit"].is_null(),
        "HEAD does not contain the staged selection"
    );
    git(&f, &["commit", "-qm", "reviewed staged change"]);
    let head = git(&f, &["rev-parse", "HEAD"]);
    f.json(&["session", "complete"]);
    let branch = f.json(&["session", "start", "--mode", "branch", "--base", &base]);
    assert_eq!(branch["review"]["comparison"]["branch"]["baseCommit"], base);
    assert_eq!(branch["review"]["comparison"]["branch"]["headCommit"], head);
    let carried = f.json(&["comments", "list"]);
    assert_eq!(carried["comments"][0]["id"], saved["comment"]["id"]);
    assert_eq!(
        carried["threads"][0]["anchor"], anchor,
        "committing and selecting a new diff never rewrites original evidence"
    );
    let exported = f.json(&["session", "export"]);
    let markdown = exported["markdown"].as_str().unwrap();
    assert!(markdown.contains("Keep this staged behavior"));
    assert!(
        !markdown.contains(&blob),
        "export is feedback, not source metadata"
    );
}

#[test]
fn should_record_confirmed_saved_file_and_ignored_directory_moves_without_changing_discussion_ids()
{
    let f = Fixture::start();
    let file = f.files.path().join("before.md");
    let moved_file = f.files.path().join("after.md");
    std::fs::write(&file, "before\nselected\nafter\n").unwrap();
    let file_path = file.to_str().unwrap();
    let moved_path = moved_file.to_str().unwrap();
    let saved = f.json(&[
        "comments",
        "add",
        "--notes",
        file_path,
        "--file",
        "before.md",
        "--line",
        "2",
        "--body",
        "Move this note",
    ]);
    let before = f.json(&["comments", "list", "--notes", file_path]);
    std::fs::rename(&file, &moved_file).unwrap();
    let movement = [
        "session",
        "refresh",
        "--notes",
        moved_path,
        "--moved-from",
        file_path,
        "--expected-version",
        before["version"].as_str().unwrap(),
        "--request-key",
        "file-move-once",
    ];
    let accepted = f.json(&movement);
    assert_eq!(
        f.json(&movement),
        accepted,
        "CLI move replay uses the original guarded request"
    );
    let moved = f.json(&["comments", "list", "--notes", moved_path]);
    assert_eq!(moved["comments"][0]["id"], saved["comment"]["id"]);
    let root = f.files.path().join(".difflab/plan/original");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("PLAN.md"), "before\nselected\nafter\n").unwrap();
    let path = root.to_str().unwrap();
    let review = f.json(&["session", "start", "--notes", path]);
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
        "Keep the plan feedback",
    ]);
    let conversation = f.json(&["comments", "list", "--notes", path]);
    let destination = f.files.path().join(".difflab/plan/renamed");
    std::fs::rename(&root, &destination).unwrap();
    let renamed = destination.to_str().unwrap();
    f.json(&[
        "session",
        "refresh",
        "--notes",
        renamed,
        "--moved-from",
        path,
        "--expected-version",
        conversation["version"].as_str().unwrap(),
        "--request-key",
        "directory-move-once",
    ]);
    let resumed = f.json(&["session", "start", "--notes", renamed]);
    assert_eq!(resumed["review"]["id"], review["review"]["id"]);
    let read = f.json(&["comments", "list", "--notes", renamed]);
    assert_eq!(read["comments"][0]["id"], comment["comment"]["id"]);
    let exported = f.json(&["session", "export", "--notes", renamed]);
    assert!(
        exported["markdown"]
            .as_str()
            .unwrap()
            .contains("Keep the plan feedback")
    );
    assert_eq!(
        f.json(&["comments", "list", "--notes", renamed]),
        read,
        "move-aware CLI export is read only"
    );
}
