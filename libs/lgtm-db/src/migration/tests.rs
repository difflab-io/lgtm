use super::super::{Database, models::identities, test_support};
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
    for (db, url, _) in test_support::databases().await {
        let identity = identities::Model {
            id: "preserved-identity".into(),
            name: "Survives migrations".into(),
            email: "alice@example.com".into(),
            model: String::new(),
            provider: String::new(),
        };
        db.identities.insert(&identity).await.unwrap();
        use super::super::entities::{comments, review_contexts, sessions, threads};
        use sea_orm::{ActiveModelTrait, Set};
        review_contexts::ActiveModel {
            id: Set("context".into()),
            kind: Set("notes".into()),
            notes_path: Set(Some("/populated".into())),
            revision: Set(0),
            refresh_status: Set("never".into()),
            ..Default::default()
        }
        .insert(db.connection())
        .await
        .unwrap();
        let session = sessions::ActiveModel {
            id: Set("session".into()),
            context_id: Set("context".into()),
            creator_id: Set(identity.id.clone()),
            state: Set("active".into()),
            created_at: Set("now".into()),
            submission_status: Set("local".into()),
            ..Default::default()
        }
        .insert(db.connection())
        .await
        .unwrap();
        threads::ActiveModel {
            id: Set("thread".into()),
            context_id: Set("context".into()),
            author_id: Set(identity.id.clone()),
            origin_session_id: Set(Some(session.id.clone())),
            anchor_kind: Set("general".into()),
            state: Set("open".into()),
            updated_at: Set("now".into()),
            outdated: Set(false),
            ..Default::default()
        }
        .insert(db.connection())
        .await
        .unwrap();
        let comment = comments::ActiveModel {
            id: Set("comment".into()),
            context_id: Set("context".into()),
            thread_id: Set("thread".into()),
            origin_session_id: Set(Some(session.id.clone())),
            author_id: Set(identity.id.clone()),
            body: Set("Survives migrations".into()),
            position: Set(0),
            visibility: Set("published".into()),
            created_at: Set("now".into()),
            updated_at: Set("now".into()),
            posting_state: Set("local".into()),
            remote_presence: Set("local".into()),
            ..Default::default()
        }
        .insert(db.connection())
        .await
        .unwrap();

        let raw = sea_orm::Database::connect(&url).await.unwrap();
        assert!(apply::<Failing>(&raw).await.is_err());
        assert!(
            !SchemaManager::new(&raw)
                .has_table("should_rollback")
                .await
                .unwrap()
        );
        assert_eq!(
            Migrator::get_applied_migrations(&raw).await.unwrap().len(),
            1
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
            2
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
            db.identities.get(&identity.id).await.unwrap(),
            Some(identity.clone())
        );
        assert_eq!(
            db.comments.get("context", &comment.id).await.unwrap(),
            Some(comment.clone())
        );
        raw.close().await.unwrap();
        db.close().await;
        let reopened = Database::connect(&url).await.unwrap();
        assert_eq!(
            reopened.identities.get(&identity.id).await.unwrap(),
            Some(identity)
        );
        assert_eq!(
            reopened.comments.get("context", &comment.id).await.unwrap(),
            Some(comment)
        );
        assert_eq!(
            reopened.sessions.get(&session.id).await.unwrap(),
            Some(session)
        );
        reopened.close().await;
    }
}
