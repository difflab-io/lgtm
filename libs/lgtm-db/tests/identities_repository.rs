//! Integration tests for `IdentitiesRepository`.

mod common;

use common::setup_db;
use lgtm_db::{DbError, IdentityKind};
use uuid::Uuid;

// fixtures --------------------------------------------------------------------

async fn seed_two_identities(db: &lgtm_db::Database) -> (lgtm_db::Identity, lgtm_db::Identity) {
    let human = db
        .identities
        .upsert_human("alice", "alice@example.com", Some("alice-gh"))
        .await
        .expect("create human");
    let agent = db
        .identities
        .upsert_agent("claude-sonnet", "anthropic", "claude-sonnet-4-5")
        .await
        .expect("create agent");
    (human, agent)
}

// tests -----------------------------------------------------------------------

mod when_upserting_and_querying_human_and_agent_identities {
    use super::*;

    #[tokio::test]
    async fn should_create_and_get_human_identity_by_id_and_name() {
        // Arrange
        let ctx = setup_db().await;

        // Act
        let created = ctx
            .db
            .identities
            .upsert_human("sid", "sid@example.com", Some("siddharthkapoor"))
            .await
            .expect("upsert human");
        let by_id = ctx
            .db
            .identities
            .get_identity(created.id)
            .await
            .expect("get by id")
            .expect("identity exists");
        let by_name = ctx
            .db
            .identities
            .get_identity_by_name("sid")
            .await
            .expect("get by name")
            .expect("identity exists");

        // Assert
        assert_eq!(created.name, "sid");
        assert_eq!(created.kind, IdentityKind::Human);
        assert_eq!(created.email.as_deref(), Some("sid@example.com"));
        assert_eq!(created.forge_login.as_deref(), Some("siddharthkapoor"));
        assert!(created.provider.is_none());
        assert!(created.model.is_none());
        assert_eq!(by_id, created);
        assert_eq!(by_name, created);
    }

    #[tokio::test]
    async fn should_update_existing_human_email_and_forge_login_on_name_conflict() {
        // Arrange
        let ctx = setup_db().await;
        let initial = ctx
            .db
            .identities
            .upsert_human("sid", "old@example.com", None)
            .await
            .expect("initial upsert");

        // Act
        let updated = ctx
            .db
            .identities
            .upsert_human("sid", "new@example.com", Some("sid-login"))
            .await
            .expect("second upsert");

        // Assert
        assert_eq!(updated.id, initial.id);
        assert_eq!(updated.email.as_deref(), Some("new@example.com"));
        assert_eq!(updated.forge_login.as_deref(), Some("sid-login"));
    }

    #[tokio::test]
    async fn should_create_and_get_agent_identity_with_provider_and_model() {
        // Arrange
        let ctx = setup_db().await;

        // Act
        let agent = ctx
            .db
            .identities
            .upsert_agent("claude-reviewer", "anthropic", "claude-3-5-sonnet")
            .await
            .expect("upsert agent");
        let updated = ctx
            .db
            .identities
            .upsert_agent("claude-reviewer", "anthropic", "claude-sonnet-4-5")
            .await
            .expect("update agent model");

        // Assert
        assert_eq!(agent.name, "claude-reviewer");
        assert_eq!(agent.kind, IdentityKind::Agent);
        assert!(agent.email.is_none());
        assert_eq!(agent.provider.as_deref(), Some("anthropic"));
        assert_eq!(agent.model.as_deref(), Some("claude-3-5-sonnet"));
        assert_eq!(updated.id, agent.id);
        assert_eq!(updated.model.as_deref(), Some("claude-sonnet-4-5"));
    }

    #[tokio::test]
    async fn should_batch_fetch_identities_by_ids() {
        // Arrange
        let ctx = setup_db().await;
        let (human, agent) = seed_two_identities(&ctx.db).await;

        // Act
        let found = ctx
            .db
            .identities
            .get_identities_by_ids(&[agent.id, Uuid::new_v4(), human.id])
            .await
            .expect("batch fetch");

        // Assert
        assert_eq!(found, vec![human, agent]);
    }
}

mod when_identity_fields_or_discriminator_rules_are_violated {
    use super::*;

    #[tokio::test]
    async fn should_reject_human_identity_with_blank_name_or_email() {
        // Arrange
        let ctx = setup_db().await;

        // Act
        let blank_name = ctx
            .db
            .identities
            .upsert_human("   ", "sid@example.com", None)
            .await;
        let blank_email = ctx.db.identities.upsert_human("sid", "   ", None).await;

        // Assert
        assert!(matches!(blank_name, Err(DbError::InvalidInput(_))));
        assert!(matches!(blank_email, Err(DbError::InvalidInput(_))));
    }

    #[tokio::test]
    async fn should_reject_agent_identity_with_blank_name_provider_or_model() {
        // Arrange
        let ctx = setup_db().await;

        // Act
        let blank_name = ctx
            .db
            .identities
            .upsert_agent("   ", "anthropic", "claude-sonnet-4-5")
            .await;
        let blank_provider = ctx
            .db
            .identities
            .upsert_agent("bot", " ", "claude-sonnet-4-5")
            .await;
        let blank_model = ctx
            .db
            .identities
            .upsert_agent("bot", "anthropic", " ")
            .await;

        // Assert
        assert!(matches!(blank_name, Err(DbError::InvalidInput(_))));
        assert!(matches!(blank_provider, Err(DbError::InvalidInput(_))));
        assert!(matches!(blank_model, Err(DbError::InvalidInput(_))));
    }

    #[tokio::test]
    async fn should_reject_upserting_agent_over_existing_human_name() {
        // Arrange
        let ctx = setup_db().await;
        let (human, _) = seed_two_identities(&ctx.db).await;

        // Act
        let conflict = ctx
            .db
            .identities
            .upsert_agent("alice", "anthropic", "claude-sonnet-4-5")
            .await;
        let preserved = ctx
            .db
            .identities
            .get_identity_by_name("alice")
            .await
            .expect("query alice")
            .expect("alice still exists");

        // Assert
        assert!(matches!(conflict, Err(DbError::Conflict(_))));
        assert_eq!(preserved, human);
    }
}
