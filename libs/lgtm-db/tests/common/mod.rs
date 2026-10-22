//! Disposable real databases for repository contracts.
use lgtm_db::Database;
#[cfg(feature = "postgres")]
use uuid::Uuid;

// fixtures --------------------------------------------------------------------
pub async fn databases() -> Vec<Database> {
    #[allow(unused_mut)]
    let mut result = vec![Database::connect("sqlite::memory:").await.unwrap()];
    #[cfg(feature = "postgres")]
    {
        use sea_orm::ConnectionTrait;
        let url = std::env::var("LGTM_TEST_POSTGRES_URL")
            .expect("PostgreSQL contracts require an explicit disposable database");
        let raw = sea_orm::Database::connect(&url).await.unwrap();
        let schema = format!("workflow_{}", Uuid::new_v4().simple());
        raw.execute_unprepared(&format!("CREATE SCHEMA {schema}"))
            .await
            .unwrap();
        raw.close().await.unwrap();
        let scoped = format!(
            "{url}{}options=-c%20search_path%3D{schema}",
            if url.contains('?') { '&' } else { '?' }
        );
        result.push(Database::connect(&scoped).await.unwrap());
    }
    result
}
