use lgtm_db::{Database, models::identities};

#[tokio::test]
async fn repositories_share_commit_rollback_and_savepoint_scope() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let identity = identities::Model {
        id: "identity-one".into(),
        name: "Alice".into(),
        email: "alice@example.com".into(),
        model: String::new(),
        provider: String::new(),
    };

    let tx = db.begin().await.unwrap();
    tx.identities.insert(&identity).await.unwrap();
    assert!(tx.identities.get(&identity.id).await.unwrap().is_some());
    tx.commit().await.unwrap();

    let mut result: identities::Model = db.identities.get(&identity.id).await.unwrap().unwrap();
    result.name = "Only in memory".to_owned();
    assert_eq!(result.name, "Only in memory");
    assert_eq!(
        db.identities.get(&identity.id).await.unwrap().unwrap().name,
        "Alice"
    );

    let mut second = identity.clone();
    second.id = "identity-two".into();
    second.email = "two@example.com".into();
    let tx = db.begin().await.unwrap();
    let nested = tx.begin().await.unwrap();
    nested.identities.insert(&second).await.unwrap();
    nested.commit().await.unwrap();
    assert!(tx.identities.get(&second.id).await.unwrap().is_some());
    tx.rollback().await.unwrap();
    assert!(db.identities.get(&second.id).await.unwrap().is_none());

    let tx = db.begin().await.unwrap();
    tx.identities.insert(&second).await.unwrap();
    drop(tx);
    assert!(db.identities.get(&second.id).await.unwrap().is_none());
    db.close().await;
}
