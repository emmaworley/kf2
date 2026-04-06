use rusqlite_migration::{M, Migrations};

use crate::config::DatabaseConfig;

pub type DbPool = deadpool_sqlite::Pool;

const MIGRATION_ARRAY: &[M] = &[M::up(
    "CREATE TABLE session (
        id TEXT PRIMARY KEY NOT NULL,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    );",
)];
const MIGRATIONS: Migrations<'static> = Migrations::from_slice(MIGRATION_ARRAY);

pub fn create_pool(config: &DatabaseConfig) -> Result<DbPool, Box<dyn std::error::Error>> {
    let cfg = deadpool_sqlite::Config::new(&config.path);
    let pool = cfg.create_pool(deadpool_sqlite::Runtime::Tokio1)?;
    Ok(pool)
}

pub async fn run_migrations(pool: &DbPool) -> Result<(), Box<dyn std::error::Error>> {
    let conn = pool.get().await?;
    conn.interact(|conn| {
        conn.pragma_update_and_check(None, "journal_mode", "WAL", |row| {
            row.get::<_, String>(0)
        })?;
        MIGRATIONS.to_latest(conn)
    })
    .await
    .map_err(|e| format!("interact error: {e}"))??;
    Ok(())
}
