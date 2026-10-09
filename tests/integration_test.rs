use anyhow::Result;
use netwatch::config::Config;
use netwatch::storage::Storage;
use tempfile::tempdir;

#[tokio::test]
async fn test_config_default() -> Result<()> {
    let cfg = Config::default();
    assert_eq!(cfg.agent.name, "netwatch");
    Ok(())
}

#[tokio::test]
async fn test_storage_init() -> Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("test.sqlite");
    let storage = Storage::connect(db_path.to_str().unwrap()).await?;
    let row = sqlx::query("SELECT name FROM sqlite_master WHERE type='table' AND name='devices'")
        .fetch_optional(&storage.pool)
        .await?;
    assert!(row.is_some());
    Ok(())
}
