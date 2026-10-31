//! Database connection pool, automatic migration runner, and client-owned transactions.

use crate::{
    error::{DbError, Result},
    migration::{Migrator, apply},
    repositories::{
        CommentsRepository, IdentitiesRepository, ReviewsRepository, SessionsRepository,
    },
};
use sea_orm::{
    ConnectOptions, ConnectionTrait, DatabaseConnection, DatabaseTransaction, Statement,
    TransactionTrait,
};
use std::{sync::Arc, time::Duration};

/// Pool-backed database client or caller-owned transaction handle exposing entity repositories.
#[derive(Clone, Debug)]
pub struct Database<C = DatabaseConnection> {
    pool: Arc<C>,
    /// Human and AI agent identity repository.
    pub identities: IdentitiesRepository<C>,
    /// Local and forge-backed review repository.
    pub reviews: ReviewsRepository<C>,
    /// Draft and published review session repository.
    pub sessions: SessionsRepository<C>,
    /// General, inline, range, and reply comment repository.
    pub comments: CommentsRepository<C>,
}

/// Caller-owned database transaction exposing the same repositories as `Database`.
pub type Transaction = Database<DatabaseTransaction>;

impl<C: ConnectionTrait> Database<C> {
    // Public ------------------------------------------------------------------

    /// Begin an atomic database transaction (or a nested savepoint when called on a `Transaction`).
    pub async fn begin(&self) -> Result<Transaction>
    where
        C: TransactionTrait<Transaction = DatabaseTransaction>,
    {
        let tx = self.pool.begin().await?;
        if tx.get_database_backend() == sea_orm::DbBackend::Sqlite {
            tx.execute_unprepared("PRAGMA foreign_keys = ON;").await?;
        }
        Ok(Database::from_connection(tx))
    }

    // Private -----------------------------------------------------------------

    fn from_connection(connection: C) -> Self {
        let pool = Arc::new(connection);
        Self {
            identities: IdentitiesRepository::new(Arc::clone(&pool)),
            reviews: ReviewsRepository::new(Arc::clone(&pool)),
            sessions: SessionsRepository::new(Arc::clone(&pool)),
            comments: CommentsRepository::new(Arc::clone(&pool)),
            pool,
        }
    }
}

impl Database<DatabaseConnection> {
    // Public ------------------------------------------------------------------

    /// Open a SQLite or PostgreSQL database connection, enforce foreign keys, and apply pending migrations.
    pub async fn connect(database_url: &str) -> Result<Self> {
        let url = database_url.trim();
        let is_sqlite = url.starts_with("sqlite:");
        let is_postgres = url.starts_with("postgres:") || url.starts_with("postgresql:");
        if !(is_sqlite || is_postgres) {
            return Err(DbError::invalid_input(
                "database_url must start with sqlite:, postgres:, or postgresql:",
            ));
        }

        let _guard = if is_sqlite && !url.contains(":memory:") {
            let path = url
                .trim_start_matches("sqlite:")
                .trim_start_matches("//")
                .split('?')
                .next()
                .unwrap_or_default();
            if !path.is_empty() {
                let lock_path = format!("{path}.migration.lock");
                Some(
                    tokio::task::spawn_blocking(move || -> std::io::Result<std::fs::File> {
                        if let Some(parent) = std::path::Path::new(&lock_path).parent()
                            && !parent.as_os_str().is_empty()
                        {
                            std::fs::create_dir_all(parent)?;
                        }
                        let file = std::fs::OpenOptions::new()
                            .create(true)
                            .read(true)
                            .write(true)
                            .truncate(false)
                            .open(lock_path)?;
                        fs2::FileExt::lock_exclusive(&file)?;
                        Ok(file)
                    })
                    .await
                    .map_err(|err| DbError::invalid_input(err.to_string()))?
                    .map_err(|err| DbError::invalid_input(err.to_string()))?,
                )
            } else {
                None
            }
        } else {
            None
        };

        let mut options = ConnectOptions::new(url);
        options
            .max_connections(if is_sqlite { 1 } else { 8 })
            .connect_timeout(Duration::from_secs(10))
            .sqlx_logging(false);

        let pool = sea_orm::Database::connect(options).await?;
        if is_sqlite && let Err(err) = pool.execute_unprepared("PRAGMA foreign_keys = ON;").await {
            let _ = pool.close().await;
            return Err(DbError::from(err));
        }
        if is_postgres
            && let Some(schema) = url
                .split("search_path%3D")
                .nth(1)
                .map(|rest| rest.split('&').next().unwrap_or_default())
                .filter(|name| {
                    !name.is_empty()
                        && name
                            .chars()
                            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
                })
            && let Err(err) = pool
                .execute_unprepared(&format!("CREATE SCHEMA IF NOT EXISTS {schema};"))
                .await
        {
            let _ = pool.close().await;
            return Err(DbError::from(err));
        }

        if let Err(err) = apply::<Migrator>(&pool).await {
            let _ = pool.close().await;
            return Err(DbError::from(err));
        }

        Ok(Self::from_connection(pool))
    }

    /// Verify that the database connection pool is reachable.
    pub async fn ping(&self) -> Result<()> {
        let backend = self.pool.get_database_backend();
        self.pool
            .execute_raw(Statement::from_string(backend, "SELECT 1"))
            .await?;
        Ok(())
    }

    /// Close the underlying connection pool.
    pub async fn close(self) -> Result<()> {
        let pool = self.pool.as_ref().clone();
        pool.close().await?;
        Ok(())
    }

    /// Return the static greeting used by the legacy `HelloReview` demonstration RPC.
    pub fn get_hello(&self) -> String {
        "Hello".to_string()
    }
}

impl Default for Database<DatabaseConnection> {
    fn default() -> Self {
        Self::from_connection(DatabaseConnection::default())
    }
}

impl Transaction {
    // Public ------------------------------------------------------------------

    /// Commit all writes performed within this transaction.
    pub async fn commit(self) -> Result<()> {
        let tx = self.into_inner()?;
        tx.commit().await.map_err(DbError::from_db_err)
    }

    /// Roll back all writes performed within this transaction.
    pub async fn rollback(self) -> Result<()> {
        let tx = self.into_inner()?;
        tx.rollback().await.map_err(DbError::from_db_err)
    }

    // Private -----------------------------------------------------------------

    fn into_inner(self) -> Result<DatabaseTransaction> {
        let Self {
            pool,
            identities,
            reviews,
            sessions,
            comments,
        } = self;
        drop((identities, reviews, sessions, comments));
        Arc::try_unwrap(pool).map_err(|_| {
            DbError::conflict("transaction cannot finalize while repository clones remain alive")
        })
    }
}
