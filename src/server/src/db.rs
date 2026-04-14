use diesel::connection::SimpleConnection;
use diesel::r2d2::{self, ConnectionManager, CustomizeConnection, Pool};
use diesel::sqlite::SqliteConnection;
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};

use crate::DatabaseConfig;

pub type DbPool = Pool<ConnectionManager<SqliteConnection>>;

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("pool build error: {0}")]
    PoolBuild(#[from] r2d2::Error),
    #[error("connection checkout error: {0}")]
    PoolCheckout(#[from] r2d2::PoolError),
    #[error("migration error: {0}")]
    Migration(String),
    #[error("blocking task canceled: {0}")]
    TaskJoin(#[from] tokio::task::JoinError),
}

/// Per-connection PRAGMAs: `foreign_keys=ON` is required for the
/// `ON DELETE CASCADE` behavior on `session_provider_config`, and
/// `journal_mode=WAL` matches the legacy pool's runtime behavior.
#[derive(Debug)]
struct SqlitePragmaCustomizer;

impl CustomizeConnection<SqliteConnection, diesel::r2d2::Error> for SqlitePragmaCustomizer {
    fn on_acquire(&self, conn: &mut SqliteConnection) -> Result<(), diesel::r2d2::Error> {
        conn.batch_execute("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")
            .map_err(r2d2::Error::QueryError)
    }
}

pub fn create_pool(config: &DatabaseConfig) -> Result<DbPool, DbError> {
    let manager = ConnectionManager::<SqliteConnection>::new(&config.path);
    let pool = Pool::builder()
        .connection_customizer(Box::new(SqlitePragmaCustomizer))
        .build(manager)?;
    Ok(pool)
}

pub async fn run_migrations(pool: &DbPool) -> Result<(), DbError> {
    let pool = pool.clone();
    tokio::task::spawn_blocking(move || -> Result<(), DbError> {
        let mut conn = pool.get()?;
        conn.run_pending_migrations(MIGRATIONS)
            .map(|_| ())
            .map_err(|e| DbError::Migration(e.to_string()))
    })
    .await??;
    Ok(())
}

#[cfg(any(test, feature = "test-support"))]
pub mod test_support {
    use super::*;

    /// SQLite URI for a named in-memory database with shared cache, so every
    /// connection opened against it sees the same database. Callers that use
    /// distinct `name`s stay isolated from each other.
    pub fn in_memory_uri(name: &str) -> String {
        format!("file:kf2_{name}?mode=memory&cache=shared")
    }

    /// Build a fully-migrated pool backed by a named in-memory SQLite database.
    /// Convenience wrapper around `in_memory_uri` + `create_pool` + `run_migrations`.
    pub async fn create_pool_in_memory(name: &str) -> Result<DbPool, DbError> {
        let cfg = DatabaseConfig {
            path: in_memory_uri(name),
        };
        let pool = create_pool(&cfg)?;
        run_migrations(&pool).await?;
        Ok(pool)
    }
}
