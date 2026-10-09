use anyhow::{Context, Result};
use sqlx::{sqlite::SqliteConnectOptions, sqlite::SqlitePoolOptions, SqlitePool};
use std::path::PathBuf;

pub struct Storage {
    pub pool: SqlitePool,
}

impl Storage {
    pub async fn connect(path: &str) -> Result<Self> {
        let file = PathBuf::from(path);
        if !file.exists() {
            std::fs::create_dir_all(file.parent().unwrap()).with_context(|| "create data dir")?;
        }
        let opts = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(opts)
            .await?;
        sqlx::migrate!()
            .run(&pool)
            .await
            .context("running migrations")?;
        Ok(Storage { pool })
    }
}
