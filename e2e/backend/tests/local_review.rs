//! Public Connect JSON endpoints served by the bundled backend process.
mod common;
use common::Fixture;
use serde_json::json;

// tests -----------------------------------------------------------------------
#[test]
fn should_persist_notes_replies_resolution_and_export_through_rpc() {
    let f = Fixture::start();
    std::fs::write(f.files.path().join("plan.md"), "one\ntwo\n").unwrap();
    let actor = f.rpc(
        "EnsureIdentity",
        json!({"name":"RPC reviewer","email":"rpc@example.invalid"}),
        None,
    );
    let identity = actor["actor"]["id"].as_str().unwrap();
    let context = f.rpc(
        "EnsureContext",
        json!({"key":{"kind":"notes","path":f.files.path()},"workspacePath":f.files.path()}),
        None,
    );
    let id = context["context"]["id"].as_str().unwrap();
    let session = f.rpc("StartSession", json!({"contextId":id}), Some(identity));
    let sid = session["session"]["id"].as_str().unwrap();
    let ws = f.rpc(
        "GetWorkspace",
        json!({"contextId":id,"includeIgnored":true}),
        None,
    );
    let hash = ws["files"][0]["contentHash"].as_str().unwrap();
    let comment=f.rpc("AddComment",json!({"sessionId":sid,"body":"Check this","anchor":{"kind":"line","path":"plan.md","start":"2","end":"2","side":"new","contentHash":hash}}),Some(identity));
    let thread = comment["comment"]["threadId"].as_str().unwrap();
    f.rpc(
        "Reply",
        json!({"sessionId":sid,"threadId":thread,"body":"Checked"}),
        Some(identity),
    );
    f.rpc(
        "ResolveThread",
        json!({"sessionId":sid,"threadId":thread,"resolved":true}),
        Some(identity),
    );
    let export = f.rpc("Export", json!({"sessionId":sid}), None);
    let text = export["markdown"].as_str().unwrap();
    assert!(text.contains("Check this") && text.contains("Checked") && text.contains("resolved"));
    let conversation = f.rpc(
        "GetConversation",
        json!({"contextId":id,"includeDrafts":true}),
        None,
    );
    assert_eq!(conversation["comments"].as_array().unwrap().len(), 2);
}
