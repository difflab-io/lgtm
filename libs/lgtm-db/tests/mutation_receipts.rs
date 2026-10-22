//! Remote intent ownership and receipt compare-and-set on both real database engines.
mod common;
use lgtm_db::{
    models::{identities, mutation_receipts, review_contexts},
    types::{ContextKind, IdentityKind},
};
use uuid::Uuid;

// fixtures --------------------------------------------------------------------
async fn owner(db: &lgtm_db::Database) -> Uuid {
    let id = Uuid::new_v4();
    db.identities
        .insert(&identities::Model {
            id,
            name: "Receipt owner".into(),
            kind: IdentityKind::Human,
            email: Some(format!("{id}@example.test")),
            model: None,
            provider: None,
            forge_account: String::new(),
        })
        .await
        .unwrap();
    id
}
async fn context(db: &lgtm_db::Database) -> Uuid {
    let id = Uuid::new_v4();
    db.review_contexts
        .insert(&review_contexts::Model {
            id,
            kind: ContextKind::Local,
            notes_path: Some(format!("/notes/{id}")),
            current_notes_path: None,
            git_dir: None,
            branch: None,
            repository_url: None,
            number: None,
            revision: 0,
            last_refreshed_at: None,
            refreshed_head: None,
            refreshed_base: None,
            refresh_error: None,
            refresh_status: "never".into(),
            forge_state: None,
        })
        .await
        .unwrap();
    id
}

// tests -----------------------------------------------------------------------
#[tokio::test]
async fn should_allow_only_one_unresolved_intent_per_object_and_preserve_accepted_receipts() {
    for db in common::databases().await {
        let context = context(&db).await;
        let first_actor = owner(&db).await;
        let second_actor = owner(&db).await;
        let object = Uuid::new_v4();
        let first = mutation_receipts::Model {
            context_id: context,
            actor_id: first_actor,
            request_key: "first".into(),
            method: "remote_edit_comment".into(),
            request_hash: "first-payload".into(),
            result_json: "prepared".into(),
            version: 1,
            object_id: Some(object),
            pending: true,
        };
        db.mutations.insert_result(&first).await.unwrap();
        let mut second = first.clone();
        second.actor_id = second_actor;
        second.request_key = "second".into();
        second.request_hash = "second-payload".into();
        assert!(
            db.mutations.insert_result(&second).await.is_err(),
            "another actor cannot claim an unresolved object"
        );
        assert!(
            db.mutations
                .get_pending(&context, Some(&object))
                .await
                .unwrap()
        );
        assert!(
            db.mutations
                .update_result(&first, "confirmed", 2, false)
                .await
                .unwrap()
        );
        assert!(
            !db.mutations
                .update_result(&first, "stale overwrite", 3, false)
                .await
                .unwrap(),
            "a stale receipt cannot replace the accepted result"
        );
        db.mutations.insert_result(&second).await.unwrap();
        assert_eq!(
            db.mutations
                .get_result(&context, &first_actor, "first")
                .await
                .unwrap()
                .unwrap()
                .result_json,
            "confirmed"
        );
        let mut no_object = first.clone();
        no_object.request_key = "missing-object".into();
        no_object.object_id = None;
        assert!(
            db.mutations.insert_result(&no_object).await.is_err(),
            "pending intent requires an object"
        );
        assert!(db.mutations.get_pending(&context, None).await.unwrap());
    }
}
