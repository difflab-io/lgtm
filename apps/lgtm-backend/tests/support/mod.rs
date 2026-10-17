use lgtm_backend::database::Database;
use lgtm_backend::{
    error::Result,
    services::{AuthService, ReviewService},
};

pub async fn databases() -> Vec<(ReviewService, String, tempfile::TempDir)> {
    let temp = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}?mode=rwc",
        temp.path().join("review.sqlite").display()
    );
    #[allow(unused_mut)]
    let mut result = vec![(service(&url).await.unwrap(), url, temp)];
    #[cfg(feature = "postgres")]
    {
        let url=std::env::var("LGTM_TEST_POSTGRES_URL").expect("postgres contract tests require LGTM_TEST_POSTGRES_URL pointing at a disposable database");
        let pool = sea_orm::Database::connect(&url).await.unwrap();
        use sea_orm::ConnectionTrait;
        let schema = format!("test_{}", uuid::Uuid::new_v4().simple());
        pool.execute_unprepared(&format!("CREATE SCHEMA {schema}"))
            .await
            .unwrap();
        pool.close().await.unwrap();
        let url = format!(
            "{url}{}options=-c%20search_path%3D{schema}",
            if url.contains('?') { '&' } else { '?' }
        );
        result.push((
            service(&url).await.unwrap(),
            url,
            tempfile::tempdir().unwrap(),
        ));
    }
    result
}

/// Build test dependencies explicitly; the service does not own connection setup.
pub async fn service(url: &str) -> Result<ReviewService> {
    let client = Database::connect(url).await?;
    let logger = tracing::Span::current();
    let auth = AuthService::new(client.clone(), logger.clone());
    Ok(ReviewService::new(client, auth, logger))
}

#[allow(dead_code)] // Used only by remote contract tests.
pub mod forge;
