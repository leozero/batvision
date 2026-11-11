use std::{path::Path, str::FromStr, time::Duration};

use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
};

pub type Db = SqlitePool;

/// Opens (creating if needed) the database in `data_dir` and applies migrations.
pub async fn open(data_dir: &Path) -> anyhow::Result<Db> {
    let options = SqliteConnectOptions::new()
        .filename(data_dir.join("batvision.db"))
        .create_if_missing(true);
    connect(options).await
}

/// In-memory database, for tests.
pub async fn open_in_memory() -> anyhow::Result<Db> {
    // A single connection: each in-memory connection would otherwise be its own database.
    let options = SqliteConnectOptions::from_str("sqlite::memory:")?;
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options.foreign_keys(true))
        .await?;
    sqlx::migrate!().run(&pool).await?;
    Ok(pool)
}

async fn connect(options: SqliteConnectOptions) -> anyhow::Result<Db> {
    let options = options
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(options)
        .await?;
    sqlx::migrate!().run(&pool).await?;
    Ok(pool)
}

/// Current time as Unix seconds.
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default()
}
