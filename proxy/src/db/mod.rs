pub mod crypto;
pub mod models;

use anyhow::{Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Pool, Sqlite};
use std::str::FromStr;

#[derive(Debug, Clone)]
pub struct Database {
    pub pool: Pool<Sqlite>,
}

impl Database {
    /// Create an in-memory SQLite database for tests/demos, with migrations applied.
    #[cfg(any(test, feature = "demo"))]
    pub async fn test_db() -> Self {
        let options = SqliteConnectOptions::from_str("sqlite::memory:")
            .expect("valid memory URL")
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);

        let pool = SqlitePoolOptions::new()
            .max_connections(1) // critical: multiple connections to :memory: get separate DBs
            .connect_with(options)
            .await
            .expect("connect to in-memory SQLite");

        let db = Self { pool };
        db.migrate().await.expect("run migrations");
        db
    }

    pub async fn connect(database_url: &str) -> Result<Self> {
        let options = SqliteConnectOptions::from_str(database_url)?
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .busy_timeout(std::time::Duration::from_secs(5));

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await
            .context("Failed to connect to SQLite database")?;

        Ok(Self { pool })
    }

    pub async fn migrate(&self) -> Result<()> {
        sqlx::migrate!("./migrations")
            .run(&self.pool)
            .await
            .context("Failed to run database migrations")?;
        Ok(())
    }
}
