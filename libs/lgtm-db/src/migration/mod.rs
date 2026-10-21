use sea_orm_migration::prelude::*;
mod m20261015_220400_review;
mod m20261018_200000_note_promotions;
mod m20261018_210000_selected_text;
mod m20261019_090000_anchor_evidence;
mod m20261019_091000_mutation_receipts;
mod m20261019_092000_forge_lifecycle;
mod m20261019_093000_notes_locations;

mod m20261020_100000_installed_schema_repair;

pub struct Migrator;
#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20261015_220400_review::Migration),
            Box::new(m20261018_200000_note_promotions::Migration),
            Box::new(m20261018_210000_selected_text::Migration),
            Box::new(m20261019_090000_anchor_evidence::Migration),
            Box::new(m20261019_091000_mutation_receipts::Migration),
            Box::new(m20261019_092000_forge_lifecycle::Migration),
            Box::new(m20261019_093000_notes_locations::Migration),
            Box::new(m20261020_100000_installed_schema_repair::Migration),
        ]
    }
}

/// Apply the framework's ordered migrations and history in a single transaction.
/// SQLite needs this wrapper; PostgreSQL's migrator nests a savepoint.
pub async fn apply<M: MigratorTrait>(db: &sea_orm::DatabaseConnection) -> Result<(), DbErr> {
    use sea_orm::TransactionTrait;
    let tx = db.begin().await?;
    M::up(&tx, None).await?;
    tx.commit().await
}

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

    use super::super::{Database, models::identities, tests::fixtures};
    use super::{Migrator, apply};
    use sea_orm_migration::prelude::*;

    struct Upgrade;

    impl MigrationName for Upgrade {
        fn name(&self) -> &str {
            "m20261016_000001_test_upgrade"
        }
    }

    #[async_trait::async_trait]
    impl MigrationTrait for Upgrade {
        async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
            m.create_table(
                Table::create()
                    .table("upgrade_fixture")
                    .col(ColumnDef::new("id").text().primary_key())
                    .to_owned(),
            )
            .await
        }

        async fn down(&self, m: &SchemaManager) -> Result<(), DbErr> {
            m.drop_table(Table::drop().table("upgrade_fixture").to_owned())
                .await
        }
    }

    struct Fail;

    impl MigrationName for Fail {
        fn name(&self) -> &str {
            "m20261017_000001_test_failure"
        }
    }

    #[async_trait::async_trait]
    impl MigrationTrait for Fail {
        async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
            m.create_table(
                Table::create()
                    .table("should_rollback")
                    .col(ColumnDef::new("id").text())
                    .to_owned(),
            )
            .await?;
            Err(DbErr::Migration("intentional failure".into()))
        }
    }

    struct Upgraded;
    #[async_trait::async_trait]
    impl MigratorTrait for Upgraded {
        fn migrations() -> Vec<Box<dyn MigrationTrait>> {
            let mut v = Migrator::migrations();
            v.push(Box::new(Upgrade));
            v
        }
    }

    struct Failing;
    #[async_trait::async_trait]
    impl MigratorTrait for Failing {
        fn migrations() -> Vec<Box<dyn MigrationTrait>> {
            let mut v = Migrator::migrations();
            v.push(Box::new(Fail));
            v
        }
    }

    #[tokio::test]
    async fn should_repair_recorded_early_schema_without_losing_reviews_or_retry_results() {
        use sea_orm::{ConnectionTrait, EntityTrait, Statement};
        for (db, url, _guard) in fixtures::isolated_databases().await {
            let raw = sea_orm::Database::connect(&url).await.unwrap();
            for table in [
                "note_promotions",
                "workspace_paths",
                "mutation_receipts",
                "comments",
                "threads",
                "sessions",
                "review_contexts",
                "repositories",
                "identities",
                "seaql_migrations",
            ] {
                raw.execute_unprepared(&format!("DROP TABLE {table}"))
                    .await
                    .unwrap();
            }
            let schema = include_str!("../tests/fixtures/installed_20261019.sql");
            let schema = if raw.get_database_backend() == sea_orm::DbBackend::Postgres {
                schema
                    .replace("uuid_text", "uuid")
                    .replace(" integer", " bigint")
            } else {
                schema.to_owned()
            };
            for statement in schema.split(';').filter(|sql| !sql.trim().is_empty()) {
                raw.execute_unprepared(statement).await.unwrap();
            }
            let actor = fixture_id("upgrade-actor");
            let context = fixture_id("upgrade-context");
            let session = fixture_id("upgrade-session");
            let thread = fixture_id("upgrade-thread");
            let comment = fixture_id("upgrade-comment");
            let [actor_sql, context_sql, session_sql, thread_sql, comment_sql] =
                [actor, context, session, thread, comment].map(|id| {
                    if raw.get_database_backend() == sea_orm::DbBackend::Sqlite {
                        format!("X'{}'", id.simple())
                    } else {
                        format!("'{id}'")
                    }
                });
            let sha = "a".repeat(40);
            let hash = format!("sha256:{}", "b".repeat(64));
            for statement in [
                format!(
                    "INSERT INTO identities(id,name,kind,email,forge_account) VALUES ({actor_sql},'Upgrade reader','human','reader@example.test','')"
                ),
                format!(
                    "INSERT INTO review_contexts(id,kind,notes_path,revision,refresh_status) VALUES ({context_sql},'local','/upgrade-fixture',7,'never')"
                ),
                format!(
                    "INSERT INTO sessions(id,context_id,creator_id,state,created_at,submission_status,comparison_kind) VALUES ({session_sql},{context_sql},{actor_sql},'active','then','local','staged')"
                ),
                format!(
                    "INSERT INTO threads(id,context_id,origin_session_id,author_id,anchor_kind,file_path,side,reviewed_commit,start_line,end_line,state,updated_at,outdated,selected_text,source_kind,source_content_hash,context_before,context_after) VALUES ({thread_sql},{context_sql},{session_sql},{actor_sql},'line','src/lib.rs','new','{sha}',2,3,'open','then',false,'original excerpt','commit','{hash}','before','after')"
                ),
                format!(
                    "INSERT INTO comments(id,context_id,thread_id,origin_session_id,author_id,owner_id,body,position,visibility,created_at,updated_at,posting_state,remote_presence) VALUES ({comment_sql},{context_sql},{thread_sql},{session_sql},{actor_sql},{actor_sql},'Preserve this draft',0,'draft','then','then','unposted','local')"
                ),
                format!(
                    "INSERT INTO note_promotions(source_comment_id,target_session_id,target_comment_id,request_key,payload_hash) VALUES ({comment_sql},{session_sql},{comment_sql},'promotion-key','hash')"
                ),
                format!(
                    "INSERT INTO mutation_receipts(context_id,actor_id,request_key,method,request_hash,result_json,version) VALUES ({context_sql},{actor_sql},'saved-key','AddComment','saved-hash','{{}}',7)"
                ),
            ] {
                raw.execute_unprepared(&statement).await.unwrap();
            }
            assert!(
                !SchemaManager::new(&raw)
                    .has_column("mutation_receipts", "object_id")
                    .await
                    .unwrap()
            );
            assert!(
                raw.execute_unprepared("UPDATE sessions SET comparison_kind = 'notes'")
                    .await
                    .is_err()
            );
            assert!(apply::<Failing>(&raw).await.is_err());
            assert!(
                !SchemaManager::new(&raw)
                    .has_column("mutation_receipts", "object_id")
                    .await
                    .unwrap(),
                "failed upgrade rolls back schema repair and history together"
            );
            apply::<Migrator>(&raw).await.unwrap();
            apply::<Migrator>(&raw).await.unwrap();
            let saved = db
                .comments
                .get_by_id(&context, &comment)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(saved.body, "Preserve this draft");
            assert!(!saved.deleted);
            assert_eq!(saved.owner_id, Some(actor));
            let saved = db
                .comments
                .get_thread(&context, &thread)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(saved.source_commit.as_deref(), Some(sha.as_str()));
            assert_eq!(saved.reviewed_commit.as_deref(), Some(sha.as_str()));
            assert_eq!(saved.source_content_hash.as_deref(), Some(hash.as_str()));
            assert_eq!(saved.selected_text.as_deref(), Some("original excerpt"));
            assert_eq!(saved.start_line, Some(2));
            assert_eq!(saved.end_line, Some(3));
            assert_eq!(
                db.sessions
                    .get_by_id(&session)
                    .await
                    .unwrap()
                    .unwrap()
                    .comparison_kind,
                crate::entities::common::ComparisonKind::Staged
            );
            let receipt = crate::entities::mutation_receipts::Entity::find()
                .one(&raw)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(receipt.request_key, "saved-key");
            assert_eq!(receipt.result_json, "{}");
            assert!(!receipt.pending);
            assert!(receipt.object_id.is_none());
            // Hidden note writers and typed file anchors must work after an actual old install.
            raw.execute_unprepared("UPDATE sessions SET comparison_kind = 'notes'")
                .await
                .unwrap();
            raw.execute_unprepared("UPDATE threads SET anchor_kind = 'file', start_line = NULL, end_line = NULL, side = NULL, reviewed_commit = NULL, selected_text = '', context_before = '', context_after = ''").await.unwrap();
            assert!(
                raw.execute_unprepared("UPDATE mutation_receipts SET pending = true")
                    .await
                    .is_err()
            );
            raw.execute_unprepared(&format!(
                "UPDATE mutation_receipts SET pending = true, object_id = {comment_sql}"
            ))
            .await
            .unwrap();
            assert!(raw.execute_unprepared(&format!("INSERT INTO mutation_receipts(context_id,actor_id,request_key,method,request_hash,result_json,version,object_id,pending) VALUES ({context_sql},{actor_sql},'duplicate','EditComment','hash','{{}}',8,{comment_sql},true)")).await.is_err());
            let promotion = raw
                .query_one_raw(Statement::from_string(
                    raw.get_database_backend(),
                    "SELECT target_comment_id FROM note_promotions",
                ))
                .await
                .unwrap()
                .unwrap();
            assert_eq!(
                promotion
                    .try_get::<uuid::Uuid>("", "target_comment_id")
                    .unwrap(),
                comment
            );
            raw.close().await.unwrap();
            db.close().await;
            let reopened = Database::connect(&url).await.unwrap();
            assert_eq!(
                reopened
                    .comments
                    .get_by_id(&context, &comment)
                    .await
                    .unwrap()
                    .unwrap()
                    .body,
                "Preserve this draft"
            );
            reopened.close().await;
        }
    }

    #[tokio::test]
    async fn shared_history_upgrade_unknown_version_and_atomic_failure_recovery() {
        for (db, url, _) in fixtures::isolated_databases().await {
            let identity = identities::Model {
                id: fixture_id("preserved-identity"),
                name: "Survives migrations".into(),
                kind: crate::entities::common::IdentityKind::Human,
                email: Some("alice@example.com".into()),
                model: None,
                provider: None,
                forge_account: String::new(),
            };
            db.identities.insert(&identity).await.unwrap();
            use super::super::entities::{comments, review_contexts, sessions, threads};
            use sea_orm::{ActiveModelTrait, Set};
            review_contexts::ActiveModel {
                id: Set(fixture_id("context")),
                kind: Set(crate::entities::common::ContextKind::Local),
                notes_path: Set(Some("/populated".into())),
                revision: Set(0),
                refresh_status: Set("never".into()),
                ..Default::default()
            }
            .insert(db.connection())
            .await
            .unwrap();
            let session = sessions::ActiveModel {
                id: Set(fixture_id("session")),
                context_id: Set(fixture_id("context")),
                creator_id: Set(identity.id),
                state: Set(crate::entities::common::SessionState::Active),
                created_at: Set("now".into()),
                submission_status: Set("local".into()),
                ..Default::default()
            }
            .insert(db.connection())
            .await
            .unwrap();
            threads::ActiveModel {
                id: Set(fixture_id("thread")),
                context_id: Set(fixture_id("context")),
                author_id: Set(identity.id),
                origin_session_id: Set(Some(session.id)),
                anchor_kind: Set("general".into()),
                state: Set("open".into()),
                updated_at: Set("now".into()),
                outdated: Set(false),
                ..Default::default()
            }
            .insert(db.connection())
            .await
            .unwrap();
            let mut comment = comments::ActiveModel {
                id: Set(fixture_id("comment")),
                context_id: Set(fixture_id("context")),
                thread_id: Set(fixture_id("thread")),
                origin_session_id: Set(Some(session.id)),
                author_id: Set(identity.id),
                body: Set("Survives migrations".into()),
                position: Set(0),
                visibility: Set("published".into()),
                created_at: Set("now".into()),
                updated_at: Set("now".into()),
                posting_state: Set(crate::entities::common::PostingState::Unposted),
                remote_presence: Set("local".into()),
                ..Default::default()
            }
            .insert(db.connection())
            .await
            .unwrap();

            let raw = sea_orm::Database::connect(&url).await.unwrap();
            // Upgrade a populated pre-evidence database, preserving legacy data without inventing a source.
            Migrator::down(&raw, Some(Migrator::migrations().len() as u32 - 3))
                .await
                .unwrap();
            assert!(
                !SchemaManager::new(&raw)
                    .has_column("threads", "source_kind")
                    .await
                    .unwrap()
            );
            apply::<Migrator>(&raw).await.unwrap();
            comment.owner_id = Some(comment.author_id);
            let preserved = db
                .comments
                .get_thread(&fixture_id("context"), &fixture_id("thread"))
                .await
                .unwrap()
                .unwrap();
            assert_eq!(preserved.source_kind, crate::types::SourceKind::Legacy);
            assert!(preserved.source_content_hash.is_none());
            assert_eq!(
                db.comments
                    .get_by_id(&fixture_id("context"), &comment.id)
                    .await
                    .unwrap(),
                Some({
                    let mut expected = comment.clone();
                    expected.owner_id = Some(expected.author_id);
                    expected
                })
            );
            assert!(apply::<Failing>(&raw).await.is_err());
            assert!(
                !SchemaManager::new(&raw)
                    .has_table("should_rollback")
                    .await
                    .unwrap()
            );
            assert_eq!(
                Migrator::get_applied_migrations(&raw).await.unwrap().len(),
                Migrator::migrations().len()
            );
            // Retrying a failure is safe, with no successful history row or partial DDL.
            assert!(apply::<Failing>(&raw).await.is_err());
            apply::<Migrator>(&raw).await.unwrap();
            apply::<Upgraded>(&raw).await.unwrap();
            assert!(
                SchemaManager::new(&raw)
                    .has_table("upgrade_fixture")
                    .await
                    .unwrap()
            );
            assert_eq!(
                Upgraded::get_applied_migrations(&raw).await.unwrap().len(),
                Migrator::migrations().len() + 1
            );
            // The exact release refuses an applied migration absent from this executable.
            assert!(Database::connect(&url).await.is_err());
            Upgraded::down(&raw, Some(1)).await.unwrap();
            assert!(
                !SchemaManager::new(&raw)
                    .has_table("upgrade_fixture")
                    .await
                    .unwrap()
            );
            // No serialization payload column; bodies and attribution are typed columns.
            assert!(
                SchemaManager::new(&raw)
                    .has_column("comments", "body")
                    .await
                    .unwrap()
            );
            assert!(
                !SchemaManager::new(&raw)
                    .has_column("comments", "payload")
                    .await
                    .unwrap()
            );
            for (table, roles) in [
                ("sessions", &["creator", "completed_by"][..]),
                ("threads", &["author", "resolved_by", "updated_by"][..]),
                ("comments", &["author"][..]),
            ] {
                for role in roles {
                    for field in ["name", "email", "model", "provider"] {
                        assert!(
                            !SchemaManager::new(&raw)
                                .has_column(table, &format!("{role}_{field}"))
                                .await
                                .unwrap()
                        );
                    }
                    assert!(
                        SchemaManager::new(&raw)
                            .has_column(table, &format!("{role}_id"))
                            .await
                            .unwrap()
                    );
                }
            }

            assert_eq!(
                db.identities.get_by_id(&identity.id).await.unwrap(),
                Some(identity.clone())
            );
            assert_eq!(
                db.comments
                    .get_by_id(&fixture_id("context"), &comment.id)
                    .await
                    .unwrap(),
                Some({
                    let mut expected = comment.clone();
                    expected.owner_id = Some(expected.author_id);
                    expected
                })
            );
            raw.close().await.unwrap();
            db.close().await;
            let reopened = Database::connect(&url).await.unwrap();
            assert_eq!(
                reopened.identities.get_by_id(&identity.id).await.unwrap(),
                Some(identity)
            );
            assert_eq!(
                reopened
                    .comments
                    .get_by_id(&fixture_id("context"), &comment.id)
                    .await
                    .unwrap(),
                Some(comment)
            );
            assert_eq!(
                reopened.sessions.get_by_id(&session.id).await.unwrap(),
                Some(session)
            );
            reopened.close().await;
        }
    }
}
