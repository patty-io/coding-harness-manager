//! SQLite connection, migrations, and repository layer.

pub mod repos;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Pool, Sqlite};
use std::str::FromStr;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DbError {
    #[error("migration failed: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("database error: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("serialize error: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("not found: {0}")]
    NotFound(String),
}

pub async fn connect(path: &str) -> Result<Pool<Sqlite>, DbError> {
    let opts = SqliteConnectOptions::from_str(path)?
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(opts)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

/// In-memory pool for tests (migrations applied to a fresh DB).
pub async fn connect_test() -> Result<Pool<Sqlite>, DbError> {
    connect("sqlite::memory:").await
}
