//! 模型配置仓储：ModelDO + Impl + From 转换。

use async_trait::async_trait;
use sqlx::{FromRow, SqlitePool};

use crate::domain::config::{ModelConfig, ModelRepository, ModelStatus};

use super::{fmt_time, now_local, parse_time};

/// 模型配置表行（cyan_model_config）
#[derive(Debug, FromRow)]
pub struct ModelDO {
    /// 主键 id
    pub id: i64,
    /// 模型名（唯一）
    pub name: String,
    /// Provider
    pub provider: String,
    /// Base URL
    pub base_url: String,
    /// API Key 引用串
    pub api_key: String,
    /// 上下文窗口
    pub context_window: i64,
    /// 是否默认
    pub is_default: i64,
    /// 状态（enabled/disabled）
    pub status: String,
    /// 创建人
    pub created_by: String,
    /// 更新人
    pub updated_by: String,
    /// 创建时间
    pub created_at: String,
    /// 更新时间
    pub updated_at: String,
    /// 删除时间（软删）
    pub deleted_at: Option<String>,
}

impl TryFrom<ModelDO> for ModelConfig {
    type Error = anyhow::Error;

    fn try_from(d: ModelDO) -> anyhow::Result<Self> {
        Ok(Self {
            id: d.id,
            name: d.name,
            provider: d.provider,
            base_url: d.base_url,
            api_key_ref: d.api_key,
            context_window: d.context_window,
            is_default: d.is_default != 0,
            status: ModelStatus::parse(&d.status),
            created_at: parse_time(&d.created_at)?,
            updated_at: parse_time(&d.updated_at)?,
        })
    }
}

const SELECT_COLS: &str =
    "id, name, provider, base_url, api_key, context_window, is_default, status,
     created_by, updated_by, created_at, updated_at, deleted_at";

/// 模型配置仓储 SQLx 实现
pub struct ModelRepositoryImpl {
    pool: SqlitePool,
}

impl ModelRepositoryImpl {
    /// 构造
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ModelRepository for ModelRepositoryImpl {
    async fn list(&self) -> anyhow::Result<Vec<ModelConfig>> {
        let rows = sqlx::query_as::<_, ModelDO>(&format!(
            "SELECT {SELECT_COLS} FROM cyan_model_config WHERE deleted_at IS NULL ORDER BY id ASC"
        ))
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(ModelConfig::try_from).collect()
    }

    async fn find_by_id(&self, id: i64) -> anyhow::Result<Option<ModelConfig>> {
        let row = sqlx::query_as::<_, ModelDO>(&format!(
            "SELECT {SELECT_COLS} FROM cyan_model_config WHERE id = ? AND deleted_at IS NULL"
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(ModelConfig::try_from).transpose()
    }

    async fn find_by_name(&self, name: &str) -> anyhow::Result<Option<ModelConfig>> {
        let row = sqlx::query_as::<_, ModelDO>(&format!(
            "SELECT {SELECT_COLS} FROM cyan_model_config WHERE name = ? AND deleted_at IS NULL"
        ))
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;
        row.map(ModelConfig::try_from).transpose()
    }

    async fn find_default(&self) -> anyhow::Result<Option<ModelConfig>> {
        let row = sqlx::query_as::<_, ModelDO>(&format!(
            "SELECT {SELECT_COLS} FROM cyan_model_config WHERE is_default = 1 AND deleted_at IS NULL"
        ))
        .fetch_optional(&self.pool)
        .await?;
        row.map(ModelConfig::try_from).transpose()
    }

    async fn insert(&self, model: &mut ModelConfig) -> anyhow::Result<()> {
        let now = now_local();
        model.created_at = now;
        model.updated_at = now;
        let id = sqlx::query(
            "INSERT INTO cyan_model_config
                (name, provider, base_url, api_key, context_window, is_default, status,
                 created_by, updated_by, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, 'local', 'local', ?, ?)",
        )
        .bind(&model.name)
        .bind(&model.provider)
        .bind(&model.base_url)
        .bind(&model.api_key_ref)
        .bind(model.context_window)
        .bind(model.is_default as i64)
        .bind(model.status.as_str())
        .bind(fmt_time(&now))
        .bind(fmt_time(&now))
        .execute(&self.pool)
        .await?
        .last_insert_rowid();
        model.id = id;
        Ok(())
    }

    async fn update(&self, model: &ModelConfig) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE cyan_model_config
             SET provider = ?, base_url = ?, api_key = ?, context_window = ?, is_default = ?,
                 status = ?, updated_by = 'local', updated_at = ?
             WHERE id = ? AND deleted_at IS NULL",
        )
        .bind(&model.provider)
        .bind(&model.base_url)
        .bind(&model.api_key_ref)
        .bind(model.context_window)
        .bind(model.is_default as i64)
        .bind(model.status.as_str())
        .bind(fmt_time(&now_local()))
        .bind(model.id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn soft_delete(&self, id: i64) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE cyan_model_config SET deleted_at = ?, updated_by = 'local', updated_at = ?
             WHERE id = ? AND deleted_at IS NULL",
        )
        .bind(fmt_time(&now_local()))
        .bind(fmt_time(&now_local()))
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn clear_default(&self) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE cyan_model_config SET is_default = 0, updated_by = 'local', updated_at = ?
             WHERE deleted_at IS NULL AND is_default = 1",
        )
        .bind(fmt_time(&now_local()))
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrations = "./migrations")]
    async fn save_backfills_id_and_default_is_unique(pool: SqlitePool) {
        let repo = ModelRepositoryImpl::new(pool);
        let mut m = ModelConfig::new(
            "kimi".into(),
            "moonshot".into(),
            "https://api.moonshot.cn/v1".into(),
            128_000,
            now_local(),
        );
        repo.insert(&mut m).await.unwrap();
        assert!(m.id > 0, "插入后应回填自增 id");

        // 设置默认后唯一
        repo.clear_default().await.unwrap();
        m.is_default = true;
        repo.update(&m).await.unwrap();
        let default = repo.find_default().await.unwrap().expect("应有默认模型");
        assert_eq!(default.name, "kimi");

        // 按名查询 + 软删过滤
        assert!(repo.find_by_name("kimi").await.unwrap().is_some());
        repo.soft_delete(m.id).await.unwrap();
        assert!(repo.find_by_name("kimi").await.unwrap().is_none());
        assert!(repo.find_default().await.unwrap().is_none());
    }
}
