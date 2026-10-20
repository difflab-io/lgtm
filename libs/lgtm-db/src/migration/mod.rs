use sea_orm_migration::prelude::*;
mod m20261015_220400_review;
mod m20261018_200000_note_promotions;
mod m20261018_210000_selected_text;
mod m20261019_090000_anchor_evidence;
mod m20261019_091000_mutation_receipts;
mod m20261019_092000_forge_lifecycle;
mod m20261019_093000_notes_locations;

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
