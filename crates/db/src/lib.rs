//! Database layer.
//!
//! Holds the SQLx connection pool, runs migrations at startup, and exposes
//! repository modules per aggregate. Higher-level crates depend on this rather
//! than on `sqlx` directly.

use sqlx::postgres::{PgPoolOptions, PgPool};
use std::time::Duration;

pub mod files;
pub mod formats;
pub mod jobs;
pub mod mappings;
pub mod merge_runs;
pub mod types;
pub mod users;

#[derive(Debug, Clone)]
pub struct DbConfig {
    pub url: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub acquire_timeout_secs: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("sqlx: {0}")]
    Sqlx(#[from] sqlx::Error),

    #[error("migrate: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
}

pub type DbResult<T> = Result<T, DbError>;

/// Embedded migrations from the workspace-root `migrations/` folder.
/// The path is relative to the *crate* manifest, so we walk up two levels.
pub static MIGRATOR: sqlx::migrate::Migrator =
    sqlx::migrate!("../../migrations");

/// Build a connection pool. The caller decides whether to run migrations.
pub async fn connect(cfg: &DbConfig) -> DbResult<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(cfg.max_connections)
        .min_connections(cfg.min_connections)
        .acquire_timeout(Duration::from_secs(cfg.acquire_timeout_secs))
        .connect(&cfg.url)
        .await?;
    Ok(pool)
}

/// Run all pending migrations.
pub async fn run_migrations(pool: &PgPool) -> DbResult<()> {
    MIGRATOR.run(pool).await?;
    Ok(())
}

/// Convert DB errors into the workspace-wide `AppError`.
impl From<DbError> for price_merger_core::AppError {
    fn from(e: DbError) -> Self {
        match &e {
            DbError::Sqlx(sqlx::Error::RowNotFound) => price_merger_core::AppError::NotFound,
            _ => price_merger_core::AppError::Database(e.to_string()),
        }
    }
}
