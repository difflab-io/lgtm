mod common;
use lgtm_db::{Database, sync::Snapshot};
use uuid::Uuid;

// tests -----------------------------------------------------------------------

#[tokio::test]
async fn should_round_trip_every_context_kind_without_copying_device_paths() {
    for db in common::databases().await {
        let target = Database::connect("sqlite::memory:").await.unwrap();
        for kind in ["notes", "branch", "forge"] {
            let snapshot = common::sync::create_snapshot(&db, kind).await;
            assert!(
                !serde_json::to_string(&snapshot)
                    .unwrap()
                    .contains("/device-a")
            );
            let tx = target.begin().await.unwrap();
            tx.sync.apply_snapshot(&snapshot).await.unwrap();
            tx.commit().await.unwrap();
            let downloaded = target.sync.get_snapshot(snapshot.context.id).await.unwrap();
            assert_eq!(
                serde_json::to_value(&downloaded).unwrap(),
                serde_json::to_value(&snapshot).unwrap()
            );
            assert_eq!(downloaded.comments[0].body, "Review this");
            assert_eq!(downloaded.file_reviews.len(), 1);
        }
    }
}

#[tokio::test]
async fn should_retry_the_exact_upload_and_keep_newer_local_edits_dirty() {
    for db in common::databases().await {
        let owner = Uuid::new_v4();
        db.sync.bind_account(owner).await.unwrap();
        let original = common::sync::create_snapshot(&db, "notes").await;
        let pending = db.sync.enqueue(original.clone()).await.unwrap().unwrap();
        let mut comment = original.comments[0].clone();
        comment.body = "Newer local edit".into();
        db.comments.update_comment(&comment).await.unwrap();
        let newer = db.sync.get_snapshot(original.context.id).await.unwrap();
        let retry = db.sync.enqueue(newer.clone()).await.unwrap().unwrap();
        assert_eq!(retry.operation_id, pending.operation_id);
        assert_eq!(
            serde_json::to_value(&retry).unwrap(),
            serde_json::to_value(&pending).unwrap()
        );
        db.sync.acknowledge(&pending, 1).await.unwrap();
        let next = db.sync.enqueue(newer).await.unwrap().unwrap();
        assert_ne!(next.operation_id, pending.operation_id);
        assert_eq!(next.expected_version, 1);
        assert_eq!(next.snapshot.comments[0].body, "Newer local edit");
    }
}

#[tokio::test]
async fn should_preserve_local_changes_on_pull_conflict_and_reject_account_switching() {
    for db in common::databases().await {
        let owner = Uuid::new_v4();
        db.sync.bind_account(owner).await.unwrap();
        assert!(db.sync.bind_account(Uuid::new_v4()).await.is_err());
        let snapshot = common::sync::create_snapshot(&db, "notes").await;
        let pending = db.sync.enqueue(snapshot.clone()).await.unwrap().unwrap();
        db.sync.acknowledge(&pending, 1).await.unwrap();
        let mut comment = snapshot.comments[0].clone();
        comment.body = "Keep this offline edit".into();
        db.comments.update_comment(&comment).await.unwrap();
        assert!(db.sync.accept(&snapshot, 2).await.is_err());
        assert_eq!(
            db.comments
                .get_comments(&snapshot.context.id)
                .await
                .unwrap()[0]
                .body,
            comment.body
        );
    }
}

#[tokio::test]
async fn should_roll_back_a_failed_snapshot_and_its_cursor() {
    for db in common::databases().await {
        db.sync.bind_account(Uuid::new_v4()).await.unwrap();
        let snapshot = common::sync::create_snapshot(&db, "forge").await;
        let mut bad: Snapshot = snapshot.clone();
        bad.comments[0].visibility = "invalid".into();
        let tx = db.begin().await.unwrap();
        tx.sync.advance_cursor(42).await.unwrap();
        assert!(tx.sync.apply_snapshot(&bad).await.is_err());
        tx.rollback().await.unwrap();
        assert_eq!(db.sync.get_cursor().await.unwrap(), 0);
        assert_eq!(
            db.comments
                .get_comments(&snapshot.context.id)
                .await
                .unwrap()[0]
                .visibility,
            "published"
        );
    }
}
