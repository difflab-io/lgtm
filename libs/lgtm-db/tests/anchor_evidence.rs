//! Source evidence, path associations, and provider locations on real database engines.
mod common;
use lgtm_db::{
    models::{identities, review_contexts, threads},
    types::{ContextKind, IdentityKind, SourceKind},
};
use uuid::Uuid;

// fixtures --------------------------------------------------------------------
async fn context(db: &lgtm_db::Database) -> (Uuid, Uuid) {
    let author = Uuid::new_v4();
    let context = Uuid::new_v4();
    db.identities
        .insert(&identities::Model {
            id: author,
            name: "Reviewer".into(),
            kind: IdentityKind::Human,
            email: Some("reviewer@example.test".into()),
            model: None,
            provider: None,
            forge_account: String::new(),
        })
        .await
        .unwrap();
    db.review_contexts
        .insert(&review_contexts::Model {
            id: context,
            kind: ContextKind::Local,
            notes_path: Some(format!("/notes/{context}")),
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
    (context, author)
}

fn thread(context_id: Uuid, author_id: Uuid) -> threads::Model {
    threads::Model {
        id: Uuid::new_v4(),
        context_id,
        origin_session_id: None,
        author_id,
        anchor_kind: "line".into(),
        file_path: Some("original.rs".into()),
        side: Some("new".into()),
        reviewed_commit: Some(format!("sha256:{}", "a".repeat(64))),
        selected_text: Some("selected".into()),
        start_line: Some(3),
        end_line: Some(3),
        state: "open".into(),
        resolved_by_id: None,
        resolved_at: None,
        updated_by_id: None,
        updated_at: "now".into(),
        forge_thread_id: None,
        outdated: false,
        source_kind: SourceKind::IndexFile,
        source_blob_oid: Some("b".repeat(40)),
        source_commit: None,
        source_content_hash: Some(format!("sha256:{}", "a".repeat(64))),
        context_before: Some("before".into()),
        context_after: Some("after".into()),
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
    }
}

// tests -----------------------------------------------------------------------
#[tokio::test]
async fn should_reject_incomplete_source_and_provider_shapes() {
    for db in common::databases().await {
        let (context, author) = context(&db).await;
        let mut missing_blob = thread(context, author);
        missing_blob.source_blob_oid = None;
        assert!(db.comments.insert_thread(&missing_blob).await.is_err());
        let mut wrong_origin = thread(context, author);
        wrong_origin.source_path = Some("/notes/not-a-git-origin".into());
        assert!(db.comments.insert_thread(&wrong_origin).await.is_err());
        let mut missing_hash = thread(context, author);
        missing_hash.source_content_hash = None;
        assert!(db.comments.insert_thread(&missing_hash).await.is_err());
        let mut missing_text = thread(context, author);
        missing_text.selected_text = None;
        assert!(db.comments.insert_thread(&missing_text).await.is_err());
        let mut partial_observation = thread(context, author);
        partial_observation.observed_start = Some(3);
        assert!(
            db.comments
                .insert_thread(&partial_observation)
                .await
                .is_err()
        );
        let mut complete = thread(context, author);
        complete.observed_outdated = Some(true);
        complete.observed_base = Some("base".into());
        complete.observed_head = Some("head".into());
        // Historical-only provider threads have no current coordinates.
        db.comments.insert_thread(&complete).await.unwrap();
        assert_eq!(
            db.comments.get_threads(&context).await.unwrap(),
            vec![complete]
        );
        db.close().await;
    }
}

#[tokio::test]
async fn should_preserve_original_evidence_when_updating_paths_and_filtering_views() {
    for db in common::databases().await {
        let (context, author) = context(&db).await;
        let original = thread(context, author);
        db.comments.insert_thread(&original).await.unwrap();
        let mut moved = original.clone();
        moved.current_path = Some("moved/file.rs".into());
        moved.file_path = Some("must-not-replace-origin.rs".into());
        moved.source_blob_oid = Some("c".repeat(40));
        moved.source_path = Some("/must-not-replace-an-origin".into());
        db.comments.update_thread(&moved).await.unwrap();
        let saved = db
            .comments
            .get_thread(&context, &original.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(saved.file_path, original.file_path);
        assert_eq!(saved.source_blob_oid, original.source_blob_oid);
        assert_eq!(saved.source_path, original.source_path);
        assert_eq!(saved.selected_text, original.selected_text);
        assert_eq!(saved.current_path.as_deref(), Some("moved/file.rs"));
        assert_eq!(
            db.comments
                .get_threads_for_paths(&context, &["moved/file.rs".into()])
                .await
                .unwrap()
                .len(),
            1
        );
        assert!(
            db.comments
                .get_threads_for_paths(&context, &["original.rs".into()])
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            db.comments
                .get_threads_for_paths(&context, &[])
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            db.comments.get_threads(&context).await.unwrap().len(),
            1,
            "hidden discussions remain in the context"
        );
        assert!(saved.orphan_reason.is_none());
        db.close().await;
    }
}

#[tokio::test]
async fn should_roll_back_path_changes_and_context_revision_on_failed_transaction() {
    for db in common::databases().await {
        let (context, author) = context(&db).await;
        let original = thread(context, author);
        db.comments.insert_thread(&original).await.unwrap();
        let tx = db.begin().await.unwrap();
        assert!(
            tx.review_contexts
                .advance_revision(&context, Some(0))
                .await
                .unwrap()
        );
        let mut moved = original.clone();
        moved.current_path = Some("moved.rs".into());
        tx.comments.update_thread(&moved).await.unwrap();
        let mut invalid = thread(context, author);
        invalid.source_blob_oid = None;
        assert!(tx.comments.insert_thread(&invalid).await.is_err());
        drop(tx);
        assert_eq!(
            db.comments
                .get_thread(&context, &original.id)
                .await
                .unwrap()
                .unwrap(),
            original
        );
        assert!(
            db.review_contexts
                .advance_revision(&context, Some(0))
                .await
                .unwrap()
        );
        assert!(
            !db.review_contexts
                .advance_revision(&context, Some(0))
                .await
                .unwrap(),
            "stale revisions cannot claim a write"
        );
        db.close().await;
    }
}

#[tokio::test]
async fn should_reject_two_contexts_claiming_the_same_current_notes_location() {
    for db in common::databases().await {
        let (id, _) = context(&db).await;
        let first = db.review_contexts.get_by_id(&id).await.unwrap().unwrap();
        let mut second = first.clone();
        second.id = Uuid::new_v4();
        second.notes_path = Some("/notes/second-source".into());
        db.review_contexts.insert(&second).await.unwrap();
        db.review_contexts
            .set_notes_location(&first.id, "/notes/current")
            .await
            .unwrap();
        assert!(
            db.review_contexts
                .set_notes_location(&second.id, "/notes/current")
                .await
                .is_err()
        );
        assert_eq!(
            db.review_contexts
                .get_by_id(&first.id)
                .await
                .unwrap()
                .unwrap()
                .current_notes_path
                .as_deref(),
            Some("/notes/current")
        );
        assert!(
            db.review_contexts
                .get_by_id(&second.id)
                .await
                .unwrap()
                .unwrap()
                .current_notes_path
                .is_none()
        );
        db.close().await;
    }
}
