use lgtm_db::types as lgtm_db_types;
use lgtm_db::{Database, models::identities};

#[tokio::test]
async fn repositories_share_commit_rollback_and_savepoint_scope() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let identity = identities::Model {
        id: fixture_id("identity-one"),
        name: "Alice".into(),
        kind: lgtm_db_types::IdentityKind::Human,
        email: Some("alice@example.com".into()),
        model: None,
        provider: None,
        forge_account: String::new(),
    };

    let tx = db.begin().await.unwrap();
    tx.identities.insert(&identity).await.unwrap();
    assert!(
        tx.identities
            .get_by_id(&identity.id)
            .await
            .unwrap()
            .is_some()
    );
    tx.commit().await.unwrap();

    let mut result: identities::Model = db
        .identities
        .get_by_id(&identity.id)
        .await
        .unwrap()
        .unwrap();
    result.name = "Only in memory".to_owned();
    assert_eq!(result.name, "Only in memory");
    assert_eq!(
        db.identities
            .get_by_id(&identity.id)
            .await
            .unwrap()
            .unwrap()
            .name,
        "Alice"
    );

    let mut second = identity.clone();
    second.id = fixture_id("identity-two");
    second.email = Some("two@example.com".into());
    let tx = db.begin().await.unwrap();
    let nested = tx.begin().await.unwrap();
    nested.identities.insert(&second).await.unwrap();
    nested.commit().await.unwrap();
    assert!(tx.identities.get_by_id(&second.id).await.unwrap().is_some());
    tx.rollback().await.unwrap();
    assert!(db.identities.get_by_id(&second.id).await.unwrap().is_none());

    let tx = db.begin().await.unwrap();
    tx.identities.insert(&second).await.unwrap();
    drop(tx);
    assert!(db.identities.get_by_id(&second.id).await.unwrap().is_none());
    db.close().await;
}

fn fixture_id(label: &str) -> uuid::Uuid {
    let mut value = 0_u128;
    for byte in label.bytes() {
        value = value.wrapping_mul(131).wrapping_add(u128::from(byte));
    }
    uuid::Uuid::from_u128(value)
}
