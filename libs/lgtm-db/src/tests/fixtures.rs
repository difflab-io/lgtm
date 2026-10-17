//! Private fixtures shared by database module tests.
use super::super::Database;

/// Open independent stores for private entity and migration tests.
///
/// Each tuple contains the client, its reconnect URL and a temporary-directory
/// guard. Keep the guard until the client is closed so SQLite files remain alive.
/// With the postgres feature, LGTM_TEST_POSTGRES_URL must identify a disposable
/// database whose user may create schemas. Each test gets a fresh schema; the
/// process/time/sequence name prevents collisions between concurrent tests.
pub async fn isolated_databases() -> Vec<(Database, String, tempfile::TempDir)> {
    let temp = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}?mode=rwc",
        temp.path().join("review.sqlite").display()
    );
    #[allow(unused_mut)] // PostgreSQL adds a second isolated database.
    let mut result = vec![(Database::connect(&url).await.unwrap(), url, temp)];
    #[cfg(feature = "postgres")]
    {
        use sea_orm::ConnectionTrait;
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT_SCHEMA: AtomicU64 = AtomicU64::new(0);
        let url = std::env::var("LGTM_TEST_POSTGRES_URL")
            .expect("postgres tests require a disposable LGTM_TEST_POSTGRES_URL");
        let raw = sea_orm::Database::connect(&url).await.unwrap();
        let schema = format!(
            "db_test_{}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT_SCHEMA.fetch_add(1, Ordering::Relaxed)
        );
        raw.execute_unprepared(&format!("CREATE SCHEMA {schema}"))
            .await
            .unwrap();
        raw.close().await.unwrap();
        let url = format!(
            "{url}{}options=-c%20search_path%3D{schema}",
            if url.contains('?') { '&' } else { '?' }
        );
        result.push((
            Database::connect(&url).await.unwrap(),
            url,
            tempfile::tempdir().unwrap(),
        ));
    }
    result
}
