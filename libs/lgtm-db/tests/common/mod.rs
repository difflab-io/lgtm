//! Shared integration test fixtures for `lgtm-db`.

#![allow(dead_code)]

use lgtm_db::{Database, Identity, NewLocalReview, Review, Session};
use tempfile::TempDir;

// fixtures --------------------------------------------------------------------

/// Isolated test database backed by a temporary SQLite file (or PostgreSQL when configured).
pub struct TestDb {
    pub db: Database,
    _temp_dir: TempDir,
}

/// Create a fresh migrated test database instance.
pub async fn setup_db() -> TestDb {
    let temp_dir = tempfile::tempdir().expect("create temp dir for test sqlite db");
    let db_path = temp_dir.path().join("test_reviews.sqlite");
    let url = std::env::var("LGTM_TEST_POSTGRES_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(|base_url| {
            let schema = format!("test_{}", uuid::Uuid::new_v4().simple());
            let separator = if base_url.contains('?') { '&' } else { '?' };
            format!("{base_url}{separator}options=-c%20search_path%3D{schema}")
        })
        .unwrap_or_else(|| format!("sqlite://{}?mode=rwc", db_path.display()));
    let db = Database::connect(&url)
        .await
        .expect("connect and migrate test database");
    TestDb {
        db,
        _temp_dir: temp_dir,
    }
}

/// Seed a standard human reviewer and local review with a draft session.
pub async fn seed_human_review_and_draft_session(db: &Database) -> (Identity, Review, Session) {
    let human = db
        .identities
        .upsert_human("sid", "sid@example.com", Some("siddharthkapoor"))
        .await
        .expect("seed human identity");
    let review = db
        .reviews
        .create_local_review(NewLocalReview {
            git_dir: "/repo/.git".to_string(),
            branch: "feature/diff-241".to_string(),
            base_sha: "1111111".to_string(),
            head_sha: "2222222".to_string(),
        })
        .await
        .expect("seed local review");
    let session = db
        .sessions
        .get_or_create_draft_session(review.id, human.id, None)
        .await
        .expect("seed draft session");
    (human, review, session)
}
