//! Notes, replies, resolution and export through the bundled public Connect JSON API.
mod common;
use common::Fixture;
use serde_json::json;
use sha2::{Digest, Sha256};

// tests -----------------------------------------------------------------------
#[test]
fn should_persist_notes_replies_resolution_and_export_through_rpc() {
    let f = Fixture::start();
    let bytes = b"one\ntwo\n";
    let file = f.files.path().join("plan.md");
    std::fs::write(&file, bytes).unwrap();
    let actor = f.rpc(
        "EnsureIdentity",
        json!({"name":"RPC reviewer","email":"rpc@example.invalid"}),
        None,
    );
    let identity = actor["actor"]["id"].as_str().unwrap();
    let scope = json!({"notes":{"path":file}});
    let comment = f.rpc("AddComment",json!({"scope":scope,"body":"Check this","expectedVersion":"0","requestKey":"note-root","anchor":{
        "kind":"line","start":"2","end":"2","side":"new","source":{"localFile":{"contentHash":format!("sha256:{:x}",Sha256::digest(bytes))}},
        "evidence":{"selectedText":"two","contextBefore":"one"}}}),Some(identity));
    let thread = &comment["comment"]["threadId"];
    let current = f.rpc("GetConversation", json!({"scope":scope}), None);
    f.rpc("Reply",json!({"scope":scope,"threadId":thread,"body":"Checked","expectedVersion":current["threads"][0]["version"],"requestKey":"note-reply"}),Some(identity));
    let current = f.rpc("GetConversation", json!({"scope":scope}), None);
    f.rpc("ResolveThread",json!({"scope":scope,"threadId":thread,"resolved":true,"expectedVersion":current["threads"][0]["version"],"requestKey":"note-resolve"}),Some(identity));
    let before = f.rpc("GetConversation", json!({"scope":scope}), None);
    let export = f.rpc("Export", json!({"scope":scope}), None);
    let text = export["markdown"].as_str().unwrap();
    assert!(text.contains("Check this") && text.contains("Checked") && text.contains("resolved"));
    assert!(text.contains("1. **[NOTE]**") && !text.contains(thread.as_str().unwrap()));
    assert_eq!(before["comments"].as_array().unwrap().len(), 2);
    assert_eq!(
        f.rpc("GetConversation", json!({"scope":scope}), None),
        before,
        "export never changes stored notes"
    );
    assert!(
        f.rpc("ListReviews", json!({}), None)["reviews"]
            .as_array()
            .is_none_or(|reviews| reviews.is_empty())
    );
}
