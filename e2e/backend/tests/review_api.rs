//! Target-based review journeys through the standalone backend's public Connect JSON API.
mod common;
use common::Fixture;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

// fixtures --------------------------------------------------------------------
fn identity(f: &Fixture) -> String {
    f.rpc(
        "EnsureIdentity",
        json!({"name":"API reviewer","email":"api@example.test"}),
        None,
    )["actor"]["id"]
        .as_str()
        .unwrap()
        .into()
}

fn hash(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn line(path: Option<&str>, bytes: &[u8]) -> Value {
    json!({"kind":"line","path":path.unwrap_or(""),"side":"new","start":"2","end":"2",
        "source":{"localFile":{"contentHash":hash(bytes)}},
        "evidence":{"selectedText":"selected","contextBefore":"before","contextAfter":"after"}})
}

fn rpc_error(f: &Fixture, method: &str, input: Value, actor: Option<&str>) -> Value {
    let (status, body) = f.response(method, input, actor);
    assert!(
        !status.is_success(),
        "{method} unexpectedly succeeded: {body}"
    );
    body
}

// tests -----------------------------------------------------------------------
#[test]
fn should_advertise_target_based_contracts_and_reject_retired_setup_operations() {
    let f = Fixture::start();
    let capabilities = f.rpc("GetCapabilities", json!({}), None);
    for required in [
        "review_targets",
        "feedback_scopes",
        "anchor_evidence",
        "diff_service",
    ] {
        assert!(
            capabilities["capabilities"]
                .as_array()
                .unwrap()
                .contains(&json!(required))
        );
    }
    let actor = identity(&f);
    for method in [
        "EnsureContext",
        "StartSession",
        "GetSession",
        "ListSessions",
        "CompleteSession",
        "RefreshContext",
        "GetWorkspace",
    ] {
        assert_eq!(
            rpc_error(&f, method, json!({}), Some(&actor))["code"],
            "unimplemented",
            "{method}"
        );
    }
    let reviews = f.rpc("ListReviews", json!({}), None);
    assert!(
        reviews["reviews"]
            .as_array()
            .is_none_or(|reviews| reviews.is_empty())
    );
}

#[test]
fn should_add_notes_by_location_and_retry_after_source_changes_without_a_visible_review() {
    let f = Fixture::start();
    let actor = identity(&f);
    let file = f.files.path().join("notes.md");
    let bytes = b"before\r\nselected\r\nafter";
    std::fs::write(&file, bytes).unwrap();
    let scope = json!({"notes":{"path":file}});

    // A read is enough to learn that there are no notes. No review setup is required.
    let empty = f.rpc("GetConversation", json!({"scope":scope}), None);
    assert_eq!(empty["version"], "0");
    assert!(
        empty["comments"]
            .as_array()
            .is_none_or(|comments| comments.is_empty())
    );
    let request = json!({"scope":scope,"anchor":line(None, bytes),"body":"Explain this case","expectedVersion":"0","requestKey":"note-create"});
    let created = f.rpc("AddComment", request.clone(), Some(&actor));
    let saved_id = created["comment"]["id"].clone();

    // A lost response may be retried even after the selected file has disappeared.
    std::fs::remove_file(&file).unwrap();
    let retried = f.rpc("AddComment", request.clone(), Some(&actor));
    assert_eq!(retried["comment"]["id"], saved_id);
    let notes = f.rpc("GetConversation", json!({"scope":scope}), None);
    assert_eq!(notes["comments"].as_array().unwrap().len(), 1);
    assert_eq!(notes["threads"][0]["anchor"]["start"], "2");
    assert_eq!(
        notes["threads"][0]["anchor"]["source"]["localFile"]["contentHash"],
        hash(bytes)
    );

    // Reusing a key for different feedback must not overwrite the saved note.
    let mut changed = request;
    changed["body"] = json!("Different request");
    assert_eq!(
        rpc_error(&f, "AddComment", changed, Some(&actor))["code"],
        "failed_precondition"
    );
    let reviews = f.rpc("ListReviews", json!({}), None);
    assert!(
        reviews["reviews"]
            .as_array()
            .is_none_or(|reviews| reviews.is_empty()),
        "ordinary notes are not visible reviews"
    );
}

#[test]
fn should_page_and_filter_one_conversation_while_preserving_moved_and_deleted_discussions() {
    let f = Fixture::start();
    let actor = identity(&f);
    let bytes = b"before\nselected\nafter\n";
    std::fs::write(f.files.path().join("one.md"), bytes).unwrap();
    std::fs::write(f.files.path().join("two.md"), "second file\n").unwrap();
    let started = f.rpc(
        "StartReview",
        json!({"target":{"directory":{"path":f.files.path()}},"requestKey":"start-directory"}),
        Some(&actor),
    );
    let review = &started["review"];
    let scope = json!({"reviewId":review["id"]});
    let root = f.rpc("AddComment", json!({"scope":scope,"anchor":line(Some("one.md"), bytes),"body":"Handle this case","expectedVersion":review["version"],"requestKey":"root"}), Some(&actor));
    let first = f.rpc("GetConversation", json!({"scope":scope}), None);
    f.rpc("Reply", json!({"scope":scope,"threadId":root["comment"]["threadId"],"body":"Keep the fallback","expectedVersion":first["threads"][0]["version"],"requestKey":"reply"}), Some(&actor));
    let replied = f.rpc("GetConversation", json!({"scope":scope}), None);
    f.rpc("AddComment", json!({"scope":scope,"anchor":{"kind":"file","path":"two.md"},"body":"Explain this file","expectedVersion":replied["version"],"requestKey":"second-file"}), Some(&actor));

    // Page boundaries may split replies. Stable IDs and order let clients combine every page.
    let mut token = String::new();
    let mut comments = Vec::new();
    loop {
        let page = f.rpc(
            "GetConversation",
            json!({"scope":scope,"pageSize":1,"pageToken":token}),
            None,
        );
        comments.extend(page["comments"].as_array().unwrap().iter().cloned());
        token = page["nextPageToken"].as_str().unwrap_or("").into();
        if token.is_empty() {
            break;
        }
    }
    assert_eq!(comments.len(), 3);
    let ids: std::collections::HashSet<_> =
        comments.iter().map(|c| c["id"].as_str().unwrap()).collect();
    assert_eq!(
        ids.len(),
        3,
        "pagination neither repeats nor loses feedback"
    );
    let visible = f.rpc(
        "GetConversation",
        json!({"scope":scope,"filePaths":["one.md"]}),
        None,
    );
    assert_eq!(visible["comments"].as_array().unwrap().len(), 2);
    assert!(
        visible["threads"][0]["orphanReason"].is_null(),
        "filtering is not orphaning"
    );

    // A confirmed path move keeps root/reply identity and the immutable original anchor.
    std::fs::create_dir(f.files.path().join("nested")).unwrap();
    std::fs::rename(
        f.files.path().join("one.md"),
        f.files.path().join("nested/one.md"),
    )
    .unwrap();
    let moved = f.rpc("RefreshReview", json!({"scope":scope,"expectedVersion":visible["version"],"requestKey":"move-file","sourceChanges":[{"moved":{"threadId":root["comment"]["threadId"],"previousPath":"one.md","currentPath":"nested/one.md","source":{"localFile":{"contentHash":hash(bytes)}}}}]}), Some(&actor));
    let at_new_path = f.rpc(
        "GetConversation",
        json!({"scope":scope,"filePaths":["nested/one.md"]}),
        None,
    );
    assert_eq!(at_new_path["comments"].as_array().unwrap().len(), 2);
    assert_eq!(at_new_path["threads"][0]["id"], root["comment"]["threadId"]);
    assert_eq!(at_new_path["threads"][0]["anchor"]["path"], "one.md");
    assert_eq!(at_new_path["threads"][0]["currentPath"], "nested/one.md");

    // Confirmed deletion retains the whole discussion in the unfiltered conversation.
    std::fs::remove_file(f.files.path().join("nested/one.md")).unwrap();
    f.rpc("RefreshReview", json!({"scope":scope,"expectedVersion":moved["version"],"requestKey":"delete-file","sourceChanges":[{"deleted":{"threadId":root["comment"]["threadId"],"path":"nested/one.md"}}]}), Some(&actor));
    let retained = f.rpc("GetConversation", json!({"scope":scope}), None);
    assert_eq!(retained["comments"].as_array().unwrap().len(), 3);
    let orphan = retained["threads"]
        .as_array()
        .unwrap()
        .iter()
        .find(|thread| thread["id"] == root["comment"]["threadId"])
        .unwrap();
    assert_eq!(orphan["orphanReason"], "file_deleted");
    assert_eq!(orphan["anchor"]["evidence"]["selectedText"], "selected");
}

#[test]
fn should_reject_stale_object_edits_and_replay_deletion_without_recreating_feedback() {
    let f = Fixture::start();
    let actor = identity(&f);
    let file = f.files.path().join("note.md");
    std::fs::write(&file, "saved file\n").unwrap();
    let scope = json!({"notes":{"path":file}});
    let created = f.rpc("AddComment", json!({"scope":scope,"body":"Original feedback","expectedVersion":"0","requestKey":"create"}), Some(&actor));
    let comment_id = &created["comment"]["id"];
    let original_token = created["comment"]["version"].as_str().unwrap();
    let edit = json!({"scope":scope,"commentId":comment_id,"body":"Accepted edit","expectedVersion":original_token,"requestKey":"first-editor"});
    let accepted = f.rpc("EditComment", edit.clone(), Some(&actor));
    assert_eq!(accepted["comment"]["body"], "Accepted edit");

    // A second editor's stale body must not replace the accepted edit.
    assert_eq!(
        rpc_error(
            &f,
            "EditComment",
            json!({"scope":scope,"commentId":comment_id,"body":"Stale edit","expectedVersion":original_token,"requestKey":"second-editor"}),
            Some(&actor)
        )["code"],
        "failed_precondition"
    );
    let current = f.rpc("GetConversation", json!({"scope":scope}), None);
    let mut combined: Vec<_> = original_token.split(':').map(str::to_owned).collect();
    combined[1] = current["version"].as_str().unwrap().into();
    assert_eq!(
        rpc_error(
            &f,
            "EditComment",
            json!({"scope":scope,"commentId":comment_id,"body":"Old body with newer review metadata","expectedVersion":combined.join(":"),"requestKey":"wrong-object-snapshot"}),
            Some(&actor)
        )["code"],
        "failed_precondition"
    );
    assert_eq!(
        f.rpc("EditComment", edit, Some(&actor))["comment"]["body"],
        "Accepted edit"
    );

    let current = f.rpc("GetConversation", json!({"scope":scope}), None);
    assert_eq!(current["comments"][0]["body"], "Accepted edit");
    let deletion = json!({"scope":scope,"commentId":comment_id,"expectedVersion":current["comments"][0]["version"],"requestKey":"delete"});
    let deleted = f.rpc("DeleteComment", deletion.clone(), Some(&actor));
    assert_eq!(
        f.rpc("DeleteComment", deletion, Some(&actor))["acceptedVersion"],
        deleted["acceptedVersion"]
    );
    let empty = f.rpc("GetConversation", json!({"scope":scope}), None);
    assert!(
        empty["comments"]
            .as_array()
            .is_none_or(|comments| comments.is_empty())
    );
    assert_eq!(empty["version"], deleted["acceptedVersion"]);
}

#[test]
fn should_replace_only_the_selected_local_attempt_and_share_its_conversation() {
    let f = Fixture::start();
    let actor = identity(&f);
    let git = |args: &[&str]| {
        let result = std::process::Command::new("git")
            .arg("-C")
            .arg(f.files.path())
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        String::from_utf8(result.stdout).unwrap().trim().to_owned()
    };
    git(&["init", "-q"]);
    std::fs::write(f.files.path().join("file.txt"), "base\n").unwrap();
    git(&["add", "file.txt"]);
    git(&[
        "-c",
        "user.name=E2E",
        "-c",
        "user.email=e2e@example.test",
        "commit",
        "-qm",
        "base",
    ]);
    let base = git(&["rev-parse", "HEAD"]);
    std::fs::write(f.files.path().join("file.txt"), "staged\n").unwrap();
    git(&["add", "file.txt"]);
    let target = json!({"localGit":{"path":f.files.path()}});
    let staged = f.rpc("StartReview", json!({"target":target,"comparison":{"staged":{"baseCommit":base}},"requestKey":"start-staged"}), Some(&actor));
    let first = &staged["review"];
    assert!(
        first["target"]["localGit"]["branch"]
            .as_str()
            .unwrap()
            .starts_with("refs/heads/")
    );
    let scope = json!({"reviewId":first["id"]});
    let feedback = f.rpc("AddComment", json!({"scope":scope,"anchor":{"kind":"file","path":"file.txt"},"body":"Keep this feedback across views","expectedVersion":first["version"],"requestKey":"feedback"}), Some(&actor));
    let conversation = f.rpc("GetConversation", json!({"scope":scope}), None);
    assert_eq!(
        rpc_error(
            &f,
            "StartReview",
            json!({"target":target,"comparison":{"working":{"baseCommit":base}},"requestKey":"different-mode"}),
            Some(&actor)
        )["code"],
        "failed_precondition"
    );
    git(&[
        "-c",
        "user.name=E2E",
        "-c",
        "user.email=e2e@example.test",
        "commit",
        "-qm",
        "commit staged content",
    ]);
    let head = git(&["rev-parse", "HEAD"]);
    let replacement = json!({"target":target,"comparison":{"branch":{"baseCommit":base,"headCommit":head}},"resumePolicy":"RESUME_POLICY_REPLACE_LOCAL","expectedReviewId":first["id"],"expectedVersion":conversation["version"],"requestKey":"switch-to-branch"});
    let branch = f.rpc("StartReview", replacement.clone(), Some(&actor));
    assert_ne!(branch["review"]["id"], first["id"]);
    assert_eq!(
        f.rpc("GetReview", json!({"reviewId":first["id"]}), None)["review"]["state"],
        "completed"
    );
    let branch_scope = json!({"reviewId":branch["review"]["id"]});
    let shared = f.rpc("GetConversation", json!({"scope":branch_scope}), None);
    assert_eq!(shared["comments"][0]["id"], feedback["comment"]["id"]);
    assert_eq!(
        f.rpc("StartReview", replacement, Some(&actor))["review"]["id"],
        branch["review"]["id"]
    );
    assert_eq!(
        f.rpc("GetConversation", json!({"scope":branch_scope}), None)["version"],
        shared["version"],
        "accepted retry must not replace the successor again"
    );
    assert_eq!(
        rpc_error(
            &f,
            "StartReview",
            json!({"target":target,"comparison":{"working":{"baseCommit":head}},"resumePolicy":"RESUME_POLICY_REPLACE_LOCAL","expectedReviewId":first["id"],"expectedVersion":shared["version"],"requestKey":"stale-attempt"}),
            Some(&actor)
        )["code"],
        "failed_precondition"
    );
    assert_eq!(
        f.rpc(
            "GetReview",
            json!({"reviewId":branch["review"]["id"]}),
            None
        )["review"]["state"],
        "active"
    );
    // A source notification from another checkout branch must not orphan the saved branch's feedback.
    let before_switch = f.rpc("GetConversation", json!({"scope":branch_scope}), None);
    git(&["checkout", "-qb", "unrelated-checkout"]);
    git(&["rm", "file.txt"]);
    assert_eq!(
        rpc_error(
            &f,
            "RefreshReview",
            json!({"scope":branch_scope,"expectedVersion":before_switch["version"],"requestKey":"wrong-branch-source","sourceChanges":[{"deleted":{"threadId":feedback["comment"]["threadId"],"path":"file.txt"}}]}),
            Some(&actor)
        )["code"],
        "failed_precondition"
    );
    assert_eq!(
        f.rpc("GetConversation", json!({"scope":branch_scope}), None),
        before_switch,
        "wrong-branch observations change no source state or version"
    );
}

#[test]
fn should_read_ignored_directory_diffs_and_derive_placements_without_starting_a_review() {
    let f = Fixture::start();
    let plans = f.files.path().join(".difflab/plan/example");
    std::fs::create_dir_all(&plans).unwrap();
    std::fs::write(f.files.path().join(".gitignore"), ".difflab/\n").unwrap();
    let original = b"before\nselected\nafter\n";
    std::fs::write(plans.join("PLAN.md"), original).unwrap();
    let diff = f.rpc(
        "GetDiff",
        json!({"target":{"directory":{"path":plans}}}),
        None,
    )["diff"]
        .clone();
    let files = f.rpc("GetFiles", json!({"diff":diff,"pageSize":1}), None);
    assert_eq!(files["files"][0]["path"], "PLAN.md");
    let exact = f.rpc("GetFile", json!({"diff":diff,"path":"PLAN.md","anchors":[{"original":line(Some("PLAN.md"), original)}]}), None);
    assert_eq!(
        exact["newSide"]["content"],
        std::str::from_utf8(original).unwrap()
    );
    assert_eq!(exact["placements"][0]["exact"]["location"]["start"], "2");

    // Supplied saved-file associations use the same absolute paths returned by review data.
    std::fs::copy(plans.join("PLAN.md"), plans.join("MOVED.md")).unwrap();
    let with_move = f.rpc(
        "GetDiff",
        json!({"target":{"directory":{"path":plans}}}),
        None,
    )["diff"]
        .clone();
    let moved_source = f.rpc("GetFile", json!({"diff":with_move,"path":"MOVED.md","anchors":[{"original":line(Some("PLAN.md"),original),"currentPath":plans.canonicalize().unwrap().join("MOVED.md")}]}), None);
    assert_eq!(
        moved_source["placements"][0]["relocated"]["location"]["start"],
        "2"
    );
    let reused = f.rpc("GetFile", json!({"diff":with_move,"path":"PLAN.md","anchors":[{"original":line(Some("PLAN.md"),original),"currentPath":"MOVED.md"}]}), None);
    assert!(
        reused["placements"][0]["unverified"].is_object(),
        "a current move association cannot reattach feedback to its reused original filename"
    );
    std::fs::remove_file(plans.join("MOVED.md")).unwrap();

    // A fresh observation can map preserved text; a stale descriptor cannot read changed content.
    std::fs::write(plans.join("PLAN.md"), "inserted\nbefore\nselected\nafter\n").unwrap();
    assert_eq!(
        rpc_error(&f, "GetFile", json!({"diff":diff,"path":"PLAN.md"}), None)["code"],
        "failed_precondition"
    );
    let fresh = f.rpc(
        "GetDiff",
        json!({"target":{"directory":{"path":plans}}}),
        None,
    )["diff"]
        .clone();
    let moved = f.rpc("GetFile", json!({"diff":fresh,"path":"PLAN.md","anchors":[{"original":line(Some("PLAN.md"), original)}]}), None);
    assert_eq!(
        moved["placements"][0]["relocated"]["location"]["start"],
        "3"
    );

    // Losing the selection produces a labeled fallback, never a guessed line.
    std::fs::write(plans.join("PLAN.md"), "unrelated replacement\n").unwrap();
    let fresh = f.rpc(
        "GetDiff",
        json!({"target":{"directory":{"path":plans}}}),
        None,
    )["diff"]
        .clone();
    let lost = f.rpc("GetFile", json!({"diff":fresh,"path":"PLAN.md","anchors":[{"original":line(Some("PLAN.md"), original)}]}), None);
    assert!(lost["placements"][0]["outdated"]["reason"].is_string());
    assert!(
        f.rpc("ListReviews", json!({}), None)["reviews"]
            .as_array()
            .is_none_or(|reviews| reviews.is_empty())
    );
}

#[test]
fn should_promote_selected_notes_and_export_without_copying_or_changing_the_source_feedback() {
    let f = Fixture::start();
    let actor = identity(&f);
    let file = f.files.path().join("note.md");
    std::fs::write(&file, "saved\n").unwrap();
    let source = json!({"notes":{"path":file}});
    let note=f.rpc("AddComment",json!({"scope":source,"body":"Promote this feedback","expectedVersion":"0","requestKey":"note"}),Some(&actor));
    let source_before = f.rpc("GetConversation", json!({"scope":source}), None);
    let review = f.rpc(
        "StartReview",
        json!({"target":{"directory":{"path":f.files.path()}},"requestKey":"review"}),
        Some(&actor),
    )["review"]
        .clone();
    let promotion = json!({"source":source,"targetReviewId":review["id"],"commentIds":[note["comment"]["id"]],"anchors":[{"kind":"file","path":"note.md"}],"expectedSourceVersion":source_before["version"],"expectedReviewVersion":review["version"],"requestKey":"promote"});
    let promoted = f.rpc("PromoteNotes", promotion.clone(), Some(&actor));
    assert_ne!(promoted["comments"][0]["id"], note["comment"]["id"]);
    assert_eq!(promoted["comments"][0]["author"], note["comment"]["author"]);
    assert!(
        promoted["acceptedVersion"]
            .as_str()
            .is_some_and(|v| !v.is_empty())
    );
    assert_eq!(
        f.rpc("PromoteNotes", promotion, Some(&actor))["comments"][0]["id"],
        promoted["comments"][0]["id"]
    );
    let source_after = f.rpc("GetConversation", json!({"scope":source}), None);
    assert_eq!(
        source_before["comments"][0]["id"],
        source_after["comments"][0]["id"]
    );
    assert_eq!(
        source_before["comments"][0]["body"],
        source_after["comments"][0]["body"]
    );
    let scope = json!({"reviewId":review["id"]});
    let before = f.rpc("GetConversation", json!({"scope":scope}), None);
    let exported = f.rpc(
        "Export",
        json!({"scope":scope,"commentIds":[promoted["comments"][0]["id"]]}),
        None,
    );
    assert!(
        exported["markdown"]
            .as_str()
            .unwrap()
            .contains("Promote this feedback")
    );
    assert_eq!(
        f.rpc("GetConversation", json!({"scope":scope}), None),
        before,
        "export is read-only"
    );
    let completion = json!({"reviewId":review["id"],"expectedVersion":before["version"],"requestKey":"complete","body":"Overall feedback"});
    let completed = f.rpc("CompleteReview", completion.clone(), Some(&actor));
    assert_eq!(completed["review"]["state"], "completed");
    assert_eq!(
        f.rpc("CompleteReview", completion, Some(&actor))["review"]["id"],
        review["id"]
    );
}

#[test]
fn should_preserve_file_level_identity_and_reconnect_only_matching_restored_notes() {
    let f = Fixture::start();
    let actor = identity(&f);
    let file = f.files.path().join("file-note.md");
    let bytes = b"original file\n";
    std::fs::write(&file, bytes).unwrap();
    let scope = json!({"notes":{"path":file}});
    let saved=f.rpc("AddComment",json!({"scope":scope,"body":"Feedback about the file","anchor":{"kind":"file","source":{"localFile":{"contentHash":hash(bytes)}},"evidence":{}},"expectedVersion":"0","requestKey":"file-note"}),Some(&actor));
    let original = f.rpc("GetConversation", json!({"scope":scope}), None);
    let thread = &original["threads"][0];
    assert_eq!(thread["anchor"]["kind"], "file");
    assert!(thread["anchor"]["start"].is_null());
    assert_eq!(
        thread["anchor"]["source"]["localFile"]["contentHash"],
        hash(bytes)
    );
    std::fs::remove_file(&file).unwrap();
    let deleted=f.rpc("RefreshReview",json!({"scope":scope,"expectedVersion":original["version"],"requestKey":"deleted","sourceChanges":[{"deleted":{"threadId":thread["id"],"path":"file-note.md"}}]}),Some(&actor));
    std::fs::write(&file, b"unrelated replacement\n").unwrap();
    assert_eq!(
        rpc_error(
            &f,
            "RefreshReview",
            json!({"scope":scope,"expectedVersion":deleted["version"],"requestKey":"wrong-restore","sourceChanges":[{"restored":{"threadId":thread["id"],"path":"file-note.md","source":{"localFile":{"contentHash":hash(b"unrelated replacement\n")}}}}]}),
            Some(&actor)
        )["code"],
        "failed_precondition"
    );
    assert_eq!(
        f.rpc("GetConversation", json!({"scope":scope}), None)["threads"][0]["orphanReason"],
        "file_deleted"
    );
    std::fs::write(&file, bytes).unwrap();
    f.rpc("RefreshReview",json!({"scope":scope,"expectedVersion":deleted["version"],"requestKey":"matching-restore","sourceChanges":[{"restored":{"threadId":thread["id"],"path":"file-note.md","source":{"localFile":{"contentHash":hash(bytes)}}}}]}),Some(&actor));
    let restored = f.rpc("GetConversation", json!({"scope":scope}), None);
    assert_eq!(restored["comments"][0]["id"], saved["comment"]["id"]);
    assert_eq!(restored["threads"][0]["anchor"], thread["anchor"]);
    assert!(
        restored["threads"][0]["orphanReason"]
            .as_str()
            .is_none_or(str::is_empty)
    );
}

#[test]
fn should_export_actionable_markdown_with_replies_lost_locations_and_overall_feedback_without_database_metadata()
 {
    let f = Fixture::start();
    let actor = identity(&f);
    let git = |args: &[&str]| {
        let result = std::process::Command::new("git")
            .arg("-C")
            .arg(f.files.path())
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        String::from_utf8(result.stdout).unwrap().trim().to_owned()
    };
    git(&["init", "-q"]);
    std::fs::create_dir(f.files.path().join("src")).unwrap();
    let original_path = "src/[x]`Ω.rs";
    let old = b"before\nselected old\n";
    let new = b"before\nselected new\n";
    std::fs::write(f.files.path().join(original_path), old).unwrap();
    std::fs::write(f.files.path().join("other.md"), "file feedback target\n").unwrap();
    git(&["add", "."]);
    git(&[
        "-c",
        "user.name=Fixture",
        "-c",
        "user.email=fixture@example.test",
        "commit",
        "-qm",
        "base",
    ]);
    let base = git(&["rev-parse", "HEAD"]);
    std::fs::write(f.files.path().join(original_path), new).unwrap();
    git(&["add", "."]);
    git(&[
        "-c",
        "user.name=Fixture",
        "-c",
        "user.email=fixture@example.test",
        "commit",
        "-qm",
        "head",
    ]);
    let head = git(&["rev-parse", "HEAD"]);
    let review=f.rpc("StartReview",json!({"target":{"localGit":{"path":f.files.path()}},"comparison":{"branch":{"baseCommit":base,"headCommit":head}},"requestKey":"start"}),Some(&actor))["review"].clone();
    let scope = json!({"reviewId":review["id"]});
    let root=f.rpc("AddComment",json!({"scope":scope,"body":"Handle this failure before returning.\n\n```rust\nreturn recover(error);\n```","anchor":{"kind":"line","path":original_path,"side":"old","start":"2","end":"2","source":{"commit":{"sha":base,"contentHash":hash(old)}},"evidence":{"selectedText":"selected old","contextBefore":"before"}},"expectedVersion":review["version"],"requestKey":"line"}),Some(&actor));
    let current = f.rpc("GetConversation", json!({"scope":scope}), None);
    f.rpc("Reply",json!({"scope":scope,"threadId":root["comment"]["threadId"],"body":"Add a regression test for the timeout case.","expectedVersion":current["threads"][0]["version"],"requestKey":"reply"}),Some(&actor));
    let current = f.rpc("GetConversation", json!({"scope":scope}), None);
    let file=f.rpc("AddComment",json!({"scope":scope,"body":"Keep this explanation in the replacement.","anchor":{"kind":"file","path":"other.md","source":{"commit":{"sha":head,"contentHash":hash(b"file feedback target\n")}},"evidence":{}},"expectedVersion":current["version"],"requestKey":"file"}),Some(&actor));
    let current = f.rpc("GetConversation", json!({"scope":scope}), None);
    let general=f.rpc("AddComment",json!({"scope":scope,"body":"Check the whole failure path.","expectedVersion":current["version"],"requestKey":"general"}),Some(&actor));
    std::fs::rename(
        f.files.path().join(original_path),
        f.files.path().join("src/moved.md"),
    )
    .unwrap();
    std::fs::remove_file(f.files.path().join("other.md")).unwrap();
    let current = f.rpc("GetConversation", json!({"scope":scope}), None);
    let moved=f.rpc("RefreshReview",json!({"scope":scope,"expectedVersion":current["version"],"requestKey":"source-events","sourceChanges":[{"moved":{"threadId":root["comment"]["threadId"],"previousPath":original_path,"currentPath":"src/moved.md","source":{"workingFile":{"contentHash":hash(new)}}}},{"deleted":{"threadId":file["comment"]["threadId"],"path":"other.md"}}]}),Some(&actor));
    f.rpc("CompleteReview",json!({"reviewId":review["id"],"expectedVersion":moved["version"],"requestKey":"complete","body":"Please address these points before merging."}),Some(&actor));
    let before = f.rpc("GetConversation", json!({"scope":scope}), None);
    let exported=f.rpc("Export",json!({"scope":scope,"commentIds":[root["comment"]["id"],file["comment"]["id"],general["comment"]["id"]]}),None);
    let markdown = exported["markdown"].as_str().unwrap();
    assert!(markdown.starts_with("# Review feedback\n\n"));
    assert_eq!(markdown.matches("**[COMMENT]**").count(), 3);
    for expected in [
        "`src/moved.md`",
        "``src/[x]`Ω.rs:~2``",
        "old (left) side",
        "Add a regression test for the timeout case.",
        "return recover(error);",
        "File deleted.",
        "> selected old",
        "## Overall feedback\n\nPlease address these points before merging.",
    ] {
        assert!(
            markdown.contains(expected),
            "missing {expected}: {markdown}"
        );
    }
    for forbidden in [
        "Session:",
        "Anchor:",
        "sha256:",
        "<!-- lgtm-",
        review["id"].as_str().unwrap(),
        root["comment"]["id"].as_str().unwrap(),
        base.as_str(),
        head.as_str(),
    ] {
        assert!(
            !markdown.contains(forbidden),
            "internal metadata in export: {markdown}"
        );
    }
    assert_eq!(
        f.rpc("GetConversation", json!({"scope":scope}), None),
        before,
        "export leaves all saved data unchanged"
    );
}

#[test]
fn should_follow_notes_between_parents_and_keep_directory_review_identity_after_root_moves() {
    let f = Fixture::start();
    let actor = identity(&f);
    let root = f.files.path().canonicalize().unwrap();
    let bytes = b"before\nselected\nafter\n";
    for name in ["notes", "moved-notes", "plan"] {
        std::fs::create_dir(root.join(name)).unwrap();
    }
    let original = root.join("notes/one.md");
    let destination = root.join("moved-notes/two.md");
    std::fs::write(&original, bytes).unwrap();
    let scope = json!({"notes":{"path":original}});
    let saved = f.rpc("AddComment", json!({"scope":scope,"anchor":line(None,bytes),"body":"Keep this note","expectedVersion":"0","requestKey":"file-note"}),Some(&actor));
    let before = f.rpc("GetConversation", json!({"scope":scope}), None);
    std::fs::copy(&original, &destination).unwrap();
    let copy = json!({"scope":scope,"expectedVersion":before["version"],"requestKey":"not-a-move","sourceChanges":[{"locationMoved":{"previousPath":original,"currentPath":destination}}]});
    assert_eq!(
        rpc_error(&f, "RefreshReview", copy, Some(&actor))["code"],
        "failed_precondition"
    );
    assert_eq!(
        f.rpc("GetConversation", json!({"scope":scope}), None),
        before,
        "rejected copy changes no version or discussion"
    );
    std::fs::remove_file(&destination).unwrap();
    std::fs::rename(&original, &destination).unwrap();
    let move_request = json!({"scope":scope,"expectedVersion":before["version"],"requestKey":"file-move","sourceChanges":[{"locationMoved":{"previousPath":original,"currentPath":destination}}]});
    let moved = f.rpc("RefreshReview", move_request.clone(), Some(&actor));
    assert_eq!(
        f.rpc("RefreshReview", move_request, Some(&actor)),
        moved,
        "move replay returns one saved result"
    );
    let new_scope = json!({"notes":{"path":destination}});
    let at_destination = f.rpc("GetConversation", json!({"scope":new_scope}), None);
    assert_eq!(at_destination["comments"][0]["id"], saved["comment"]["id"]);
    assert_eq!(
        at_destination["threads"][0]["sourcePath"],
        original.to_string_lossy().as_ref()
    );
    assert_eq!(
        at_destination["threads"][0]["currentPath"],
        destination.to_string_lossy().as_ref()
    );
    f.rpc("AddComment",json!({"scope":new_scope,"anchor":line(None,bytes),"body":"New note at destination","expectedVersion":at_destination["version"],"requestKey":"new-location-note"}),Some(&actor));
    let plan = root.join("plan");
    std::fs::write(plan.join("PLAN.md"), bytes).unwrap();
    let started = f.rpc(
        "StartReview",
        json!({"target":{"directory":{"path":plan}},"requestKey":"directory-start"}),
        Some(&actor),
    );
    let review_scope = json!({"reviewId":started["review"]["id"]});
    let comment = f.rpc("AddComment",json!({"scope":review_scope,"anchor":line(Some("PLAN.md"),bytes),"body":"Plan feedback","expectedVersion":started["review"]["version"],"requestKey":"plan-feedback"}),Some(&actor));
    let read = f.rpc("GetConversation", json!({"scope":review_scope}), None);
    let renamed = root.join("renamed-plan");
    std::fs::rename(&plan, &renamed).unwrap();
    f.rpc("RefreshReview",json!({"scope":review_scope,"expectedVersion":read["version"],"requestKey":"plan-root-move","sourceChanges":[{"locationMoved":{"previousPath":plan,"currentPath":renamed}}]}),Some(&actor));
    let review = f.rpc(
        "GetReview",
        json!({"reviewId":started["review"]["id"]}),
        None,
    );
    assert_eq!(
        review["review"]["target"]["directory"]["path"],
        renamed.to_string_lossy().as_ref()
    );
    let resumed = f.rpc(
        "StartReview",
        json!({"target":{"directory":{"path":renamed}},"requestKey":"resume-moved-plan"}),
        Some(&actor),
    );
    assert_eq!(resumed["review"]["id"], started["review"]["id"]);
    let filtered = f.rpc(
        "GetConversation",
        json!({"scope":review_scope,"filePaths":["PLAN.md"]}),
        None,
    );
    assert_eq!(filtered["comments"][0]["id"], comment["comment"]["id"]);
    assert_eq!(
        filtered["threads"][0]["anchor"],
        read["threads"][0]["anchor"]
    );
}
