use super::super::models::{identities, review_contexts};
use super::threads;

#[tokio::test]
async fn dense_relations_keep_identity_roles_and_optional_links_distinct() {
    for (db, _, _) in super::super::test_support::databases().await {
        for id in ["author", "resolver", "updater"] {
            db.identities
                .insert(&identities::Model {
                    id: id.into(),
                    name: id.into(),
                    email: format!("{id}@example.com"),
                    model: String::new(),
                    provider: String::new(),
                })
                .await
                .unwrap();
        }
        db.review_contexts
            .insert(&review_contexts::Model {
                id: "context".into(),
                kind: "notes".into(),
                notes_path: Some("/notes".into()),
                repository: None,
                branch: None,
                host: None,
                number: None,
                revision: 0,
                last_refreshed_at: None,
                refreshed_head: None,
                refreshed_base: None,
                refresh_error: None,
                refresh_status: "never".into(),
            })
            .await
            .unwrap();
        let mut thread = threads::Model {
            id: "thread".into(),
            context_id: "context".into(),
            origin_session_id: None,
            author_id: "author".into(),
            anchor_kind: "general".into(),
            file_path: None,
            side: None,
            reviewed_commit: None,
            start_line: None,
            end_line: None,
            state: "resolved".into(),
            resolved_by_id: Some("resolver".into()),
            resolved_at: Some("now".into()),
            updated_by_id: Some("updater".into()),
            updated_at: "now".into(),
            forge_thread_id: None,
            outdated: false,
        };
        db.comments.insert_thread(&thread).await.unwrap();
        let loaded = threads::Entity::load()
            .with(threads::Relation::Author)
            .with(threads::Relation::ResolvedBy)
            .with(threads::Relation::UpdatedBy)
            .with(super::review_contexts::Entity)
            .with(super::sessions::Entity)
            .one(db.connection())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(loaded.author.as_ref().unwrap().id, "author");
        assert_eq!(loaded.resolved_by.as_ref().unwrap().id, "resolver");
        assert_eq!(loaded.updated_by.as_ref().unwrap().id, "updater");
        assert_eq!(loaded.context.as_ref().unwrap().id, "context");
        assert!(loaded.origin_session.as_ref().is_none());

        thread.state = "open".into();
        thread.resolved_by_id = None;
        thread.resolved_at = None;
        db.comments.update_thread(&thread).await.unwrap();
        let reopened = threads::Entity::load()
            .with(threads::Relation::ResolvedBy)
            .one(db.connection())
            .await
            .unwrap()
            .unwrap();
        assert!(reopened.resolved_by.as_ref().is_none());
        db.close().await;
    }
}
