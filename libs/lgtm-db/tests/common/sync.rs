use lgtm_db::{
    Database,
    models::{identities, review_contexts},
    sync::Snapshot,
    types::{ContextKind, IdentityKind},
};
use uuid::Uuid;

// fixtures --------------------------------------------------------------------

pub async fn create_snapshot(db: &Database, kind: &str) -> Snapshot {
    let actor = identities::Model {
        id: Uuid::new_v4(),
        name: "Alice".into(),
        kind: IdentityKind::Human,
        email: Some(format!("{}@example.test", Uuid::new_v4())),
        model: None,
        provider: None,
        forge_account: String::new(),
    };
    db.identities.insert(&actor).await.unwrap();
    let id = Uuid::new_v4();
    let context = review_contexts::Model {
        id,
        kind: if kind == "forge" {
            ContextKind::ForgeBacked
        } else {
            ContextKind::Local
        },
        notes_path: (kind == "notes").then(|| format!("/device-a/{id}/notes")),
        current_notes_path: None,
        git_dir: (kind == "branch").then(|| format!("/device-a/{id}/.git")),
        branch: (kind == "branch").then(|| "refs/heads/feature".into()),
        repository_url: (kind == "forge").then(|| "https://github.com/example/repo".into()),
        number: (kind == "forge").then_some(1),
        revision: 0,
        last_refreshed_at: None,
        refreshed_head: None,
        refreshed_base: None,
        refresh_error: None,
        refresh_status: "never".into(),
        forge_state: None,
    };
    db.review_contexts.insert(&context).await.unwrap();
    let thread_id = Uuid::new_v4();
    let thread = serde_json::from_value(serde_json::json!({
        "id": thread_id, "context_id": id, "author_id": actor.id, "anchor_kind": "general",
        "state": "open", "updated_at": "2026-10-26T00:00:00Z"
    }))
    .unwrap();
    db.comments.insert_thread(&thread).await.unwrap();
    let comment = serde_json::from_value(serde_json::json!({
        "id": Uuid::new_v4(), "context_id": id, "thread_id": thread_id, "author_id": actor.id,
        "owner_id": actor.id, "body": "Review this", "position": 0, "visibility": "published",
        "created_at": "2026-10-26T00:00:00Z", "updated_at": "2026-10-26T00:00:00Z", "posting_state": "unposted"
    })).unwrap();
    db.comments.insert_comment(&comment).await.unwrap();
    db.review_contexts
        .set_file_reviewed(&id, &actor.id, "src/main.rs", Some("sha256:example"))
        .await
        .unwrap();
    db.sync.get_snapshot(id).await.unwrap()
}
