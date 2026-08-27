//! 数据源：`~/.cyan/cyan.db` 连接池 + migration。

use std::path::PathBuf;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;

/// cyan 数据目录（`~/.cyan`）
pub fn cyan_home() -> anyhow::Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("无法定位用户主目录"))?;
    Ok(home.join(".cyan"))
}

/// 数据库文件路径（`~/.cyan/cyan.db`）
pub fn db_path() -> anyhow::Result<PathBuf> {
    Ok(cyan_home()?.join("cyan.db"))
}

/// 建库（create_if_missing）并跑 migration，返回连接池
pub async fn init_pool() -> anyhow::Result<SqlitePool> {
    let dir = cyan_home()?;
    std::fs::create_dir_all(&dir)?;
    let path = db_path()?;
    let options = SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(options)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    tracing::info!(db = %path.display(), "数据库初始化完成");
    Ok(pool)
}
