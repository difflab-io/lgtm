//! Connections, migrations and client-owned SeaORM transactions.
use super::repositories::*;
use super::{
    error::{Result, invalid},
    migration::Migrator,
};
use sea_orm::{
    ConnectOptions, ConnectionTrait, DatabaseConnection, DatabaseTransaction, TransactionTrait,
};
use std::{sync::Arc, time::Duration};

/// Pool-backed database client, or repositories sharing one caller-owned transaction.
#[derive(Clone)]
pub struct Database<C = DatabaseConnection> {
    pool: Arc<C>,
    /// Human and agent identity operations.
    pub identities: IdentityRepository<C>,
    /// Review session operations.
    pub sessions: SessionRepository<C>,
    /// Context identity and refresh operations.
    pub review_contexts: ReviewContextRepository<C>,
    /// Forge repository URLs and canonical workspace paths.
    pub workspace: WorkspaceRepository<C>,
    /// Thread, root comment and reply operations.
    pub comments: CommentRepository<C>,
    /// Source note provenance and idempotent promotion receipts.
    pub promotions: PromotionRepository<C>,
    /// Atomically saved results for local mutation retries.
    pub mutations: MutationRepository<C>,
}

/// Client-owned transaction exposing the same repositories as a pool-backed client.
pub type Transaction = Database<DatabaseTransaction>;

impl<C: ConnectionTrait + TransactionTrait> Database<C> {
    fn from_connection(connection: C) -> Self {
        let pool = Arc::new(connection);
        Self {
            identities: IdentityRepository::new(Arc::clone(&pool)),
            sessions: SessionRepository::new(Arc::clone(&pool)),
            review_contexts: ReviewContextRepository::new(Arc::clone(&pool)),
            workspace: WorkspaceRepository::new(Arc::clone(&pool)),
            comments: CommentRepository::new(Arc::clone(&pool)),
            promotions: PromotionRepository::new(Arc::clone(&pool)),
            mutations: MutationRepository::new(Arc::clone(&pool)),
            pool,
        }
    }
}

impl Database {
    /// Open a supported database and apply its private migration history. Returns an error for invalid URLs, unavailable drivers, connection failures or migration failures.
    pub async fn connect(url: &str) -> Result<Self> {
        if !(url.starts_with("sqlite:")
            || url.starts_with("postgres:")
            || url.starts_with("postgresql:"))
        {
            return Err(invalid("Database URL must use sqlite: or postgres:"));
        }
        // Cross-process serialization only for SQLite startup; PostgreSQL deployments run one migration job.
        let _guard = if url.starts_with("sqlite:") && !url.contains(":memory:") {
            let path = url
                .trim_start_matches("sqlite:")
                .trim_start_matches("//")
                .split('?')
                .next()
                .unwrap_or_default();
            let name = format!("{path}.migration.lock");
            Some(
                tokio::task::spawn_blocking(move || -> std::io::Result<std::fs::File> {
                    let f = std::fs::OpenOptions::new()
                        .create(true)
                        .read(true)
                        .write(true)
                        .truncate(false)
                        .open(name)?;
                    fs2::FileExt::lock_exclusive(&f)?;
                    Ok(f)
                })
                .await
                .map_err(|e| invalid(e.to_string()))??,
            )
        } else {
            None
        };
        let mut options = ConnectOptions::new(url);
        options
            .max_connections(if url.starts_with("sqlite:") { 1 } else { 8 })
            .connect_timeout(Duration::from_secs(10))
            .sqlx_logging(false);
        let pool = sea_orm::Database::connect(options).await?;
        // An explicit transaction gives SQLite the atomic DDL/history behavior not supplied by the migrator itself.
        if let Err(error) = super::migration::apply::<Migrator>(&pool).await {
            // Close startup connections before returning a migration error.
            let _ = pool.close().await;
            return Err(error.into());
        }
        Ok(Self::from_connection(pool))
    }

    /// Close the shared connection pool. Other cloned clients share the same pool.
    pub async fn close(&self) {
        let _ = self.pool.as_ref().clone().close().await;
    }
}

impl<C: ConnectionTrait> Database<C> {
    #[cfg(test)]
    pub(crate) fn connection(&self) -> &C {
        self.pool.as_ref()
    }
}

impl<C: ConnectionTrait + TransactionTrait<Transaction = DatabaseTransaction>> Database<C> {
    /// Begin an atomic transaction, or a nested savepoint when called on a transaction.
    pub async fn begin(&self) -> Result<Transaction> {
        Ok(Database::from_connection(self.pool.begin().await?))
    }
}

impl Database<DatabaseTransaction> {
    fn into_transaction(self) -> Result<DatabaseTransaction> {
        let Self {
            pool,
            identities,
            sessions,
            review_contexts,
            workspace,
            comments,
            promotions,
            mutations,
        } = self;
        drop((
            identities,
            sessions,
            review_contexts,
            workspace,
            comments,
            promotions,
            mutations,
        ));
        Arc::try_unwrap(pool)
            .map_err(|_| invalid("Transaction still has repository handles in use"))
    }

    /// Commit this transaction. Returns an error if commit fails or repository handles escaped the owner.
    pub async fn commit(self) -> Result<()> {
        Ok(self.into_transaction()?.commit().await?)
    }

    /// Roll back this transaction. Dropping an uncommitted transaction also rolls it back.
    pub async fn rollback(self) -> Result<()> {
        Ok(self.into_transaction()?.rollback().await?)
    }
}
