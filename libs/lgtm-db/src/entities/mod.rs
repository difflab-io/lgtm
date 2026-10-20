pub mod comments;
pub mod common;
pub mod identities;
pub mod mutation_receipts;
pub mod note_promotions;
pub mod repositories;
pub mod review_contexts;
pub mod sessions;
pub mod threads;
pub mod workspace_paths;

// tests -----------------------------------------------------------------------
#[cfg(test)]
mod tests {
    fn fixture_id(label: &str) -> uuid::Uuid {
        let mut value = 0_u128;
        for byte in label.bytes() {
            value = value.wrapping_mul(131).wrapping_add(u128::from(byte));
        }
        uuid::Uuid::from_u128(value)
    }

    use super::super::models::{identities, review_contexts};
    use super::threads;

    #[tokio::test]
    async fn dense_relations_keep_identity_roles_and_optional_links_distinct() {
        for (db, _, _) in super::super::tests::fixtures::isolated_databases().await {
            for id in ["author", "resolver", "updater"] {
                db.identities
                    .insert(&identities::Model {
                        id: fixture_id(id),
                        name: id.into(),
                        kind: crate::entities::common::IdentityKind::Human,
                        email: Some(format!("{id}@example.com")),
                        model: None,
                        provider: None,
                        forge_account: String::new(),
                    })
                    .await
                    .unwrap();
            }
            db.review_contexts
                .insert(&review_contexts::Model {
                    id: fixture_id("context"),
                    kind: crate::entities::common::ContextKind::Local,
                    notes_path: Some("/notes".into()),
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
            let mut thread = threads::Model {
                id: fixture_id("thread"),
                context_id: fixture_id("context"),
                origin_session_id: None,
                author_id: fixture_id("author"),
                anchor_kind: "general".into(),
                file_path: None,
                side: None,
                reviewed_commit: None,
                selected_text: None,
                source_kind: crate::types::SourceKind::Legacy,
                source_blob_oid: None,
                source_commit: None,
                source_content_hash: None,
                context_before: None,
                context_after: None,
                current_path: None,
                source_path: None,
                orphan_reason: None,
                observed_base: None,
                observed_head: None,
                observed_outdated: None,
                observed_path: None,
                observed_side: None,
                observed_start: None,
                observed_end: None,
                observed_commit: None,

                start_line: None,
                end_line: None,
                state: "resolved".into(),
                resolved_by_id: Some(fixture_id("resolver")),
                resolved_at: Some("now".into()),
                updated_by_id: Some(fixture_id("updater")),
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
            assert_eq!(loaded.author.as_ref().unwrap().id, fixture_id("author"));
            assert_eq!(
                loaded.resolved_by.as_ref().unwrap().id,
                fixture_id("resolver")
            );
            assert_eq!(
                loaded.updated_by.as_ref().unwrap().id,
                fixture_id("updater")
            );
            assert_eq!(loaded.context.as_ref().unwrap().id, fixture_id("context"));
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
}
