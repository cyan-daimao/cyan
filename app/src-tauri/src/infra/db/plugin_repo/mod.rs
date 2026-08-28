//! 插件仓储：PluginDO + Impl + From 转换。

use async_trait::async_trait;
use sqlx::{FromRow, SqlitePool};

use crate::domain::plugin::{Plugin, PluginRepository, PluginStatus};

use super::{fmt_time, now_local, parse_time};

/// 插件表行（cyan_plugin）
#[derive(Debug, FromRow)]
pub struct PluginDO {
    /// 主键 id
    pub id: i64,
    /// 插件名（唯一）
    pub name: String,
    /// 版本
    pub version: String,
    /// 作者
    pub author: String,
    /// 描述
    pub description: String,
    /// 状态（enabled/disabled）
    pub status: String,
    /// 携带技能数
    pub skill_count: i64,
    /// 携带 MCP 服务器数
    pub mcp_count: i64,
    /// 携带权限规则数
    pub rule_count: i64,
    /// 创建人
    pub created_by: String,
    /// 更新人
    pub updated_by: String,
    /// 安装时间
    pub created_at: String,
    /// 更新时间
    pub updated_at: String,
    /// 删除时间（软删）
    pub deleted_at: Option<String>,
}

impl TryFrom<PluginDO> for Plugin {
    type Error = anyhow::Error;

    fn try_from(d: PluginDO) -> anyhow::Result<Self> {
        Ok(Self {
            id: d.id,
            name: d.name,
            version: d.version,
            author: d.author,
            description: d.description,
            status: PluginStatus::parse(&d.status),
            skill_count: d.skill_count,
            mcp_count: d.mcp_count,
            rule_count: d.rule_count,
            created_at: parse_time(&d.created_at)?,
            updated_at: parse_time(&d.updated_at)?,
        })
    }
}

const SELECT_COLS: &str =
    "id, name, version, author, description, status, skill_count, mcp_count, rule_count,
     created_by, updated_by, created_at, updated_at, deleted_at";

/// 插件仓储 SQLx 实现
pub struct PluginRepositoryImpl {
    pool: SqlitePool,
}

impl PluginRepositoryImpl {
    /// 构造
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PluginRepository for PluginRepositoryImpl {
    async fn list(&self) -> anyhow::Result<Vec<Plugin>> {
        let rows = sqlx::query_as::<_, PluginDO>(&format!(
            "SELECT {SELECT_COLS} FROM cyan_plugin WHERE deleted_at IS NULL ORDER BY created_at DESC, id DESC"
        ))
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(Plugin::try_from).collect()
    }

    async fn find_by_id(&self, id: i64) -> anyhow::Result<Option<Plugin>> {
        let row = sqlx::query_as::<_, PluginDO>(&format!(
            "SELECT {SELECT_COLS} FROM cyan_plugin WHERE id = ? AND deleted_at IS NULL"
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(Plugin::try_from).transpose()
    }

    async fn find_by_name(&self, name: &str) -> anyhow::Result<Option<Plugin>> {
        let row = sqlx::query_as::<_, PluginDO>(&format!(
            "SELECT {SELECT_COLS} FROM cyan_plugin WHERE name = ? AND deleted_at IS NULL"
        ))
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;
        row.map(Plugin::try_from).transpose()
    }

    async fn insert(&self, plugin: &mut Plugin) -> anyhow::Result<()> {
        let now = now_local();
        plugin.created_at = now;
        plugin.updated_at = now;
        let id = sqlx::query(
            "INSERT INTO cyan_plugin
                (name, version, author, description, status, skill_count, mcp_count, rule_count,
                 created_by, updated_by, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'local', 'local', ?, ?)",
        )
        .bind(&plugin.name)
        .bind(&plugin.version)
        .bind(&plugin.author)
        .bind(&plugin.description)
        .bind(plugin.status.as_str())
        .bind(plugin.skill_count)
        .bind(plugin.mcp_count)
        .bind(plugin.rule_count)
        .bind(fmt_time(&now))
        .bind(fmt_time(&now))
        .execute(&self.pool)
        .await?
        .last_insert_rowid();
        plugin.id = id;
        Ok(())
    }

    async fn update(&self, plugin: &Plugin) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE cyan_plugin
             SET version = ?, author = ?, description = ?, status = ?,
                 skill_count = ?, mcp_count = ?, rule_count = ?,
                 updated_by = 'local', updated_at = ?
             WHERE id = ? AND deleted_at IS NULL",
        )
        .bind(&plugin.version)
        .bind(&plugin.author)
        .bind(&plugin.description)
        .bind(plugin.status.as_str())
        .bind(plugin.skill_count)
        .bind(plugin.mcp_count)
        .bind(plugin.rule_count)
        .bind(fmt_time(&now_local()))
        .bind(plugin.id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn soft_delete(&self, id: i64) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE cyan_plugin SET deleted_at = ?, updated_by = 'local', updated_at = ?
             WHERE id = ? AND deleted_at IS NULL",
        )
        .bind(fmt_time(&now_local()))
        .bind(fmt_time(&now_local()))
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::plugin::PluginManifest;

    fn manifest(name: &str) -> PluginManifest {
        PluginManifest {
            name: name.into(),
            version: "1.0.0".into(),
            author: "a".into(),
            description: "d".into(),
            cyan_min_version: None,
            permissions: vec!["skills".into()],
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn insert_backfills_id_and_soft_delete_filters(pool: SqlitePool) {
        let repo = PluginRepositoryImpl::new(pool);
        let mut p = Plugin::from_manifest(&manifest("demo-plugin"), (2, 1, 3), now_local());
        repo.insert(&mut p).await.unwrap();
        assert!(p.id > 0);

        let found = repo.find_by_name("demo-plugin").await.unwrap().expect("应能查到");
        assert_eq!(found.skill_count, 2);
        assert_eq!(found.status, PluginStatus::Enabled);

        let mut found = found;
        found.disable();
        repo.update(&found).await.unwrap();
        let found = repo.find_by_id(p.id).await.unwrap().unwrap();
        assert_eq!(found.status, PluginStatus::Disabled);

        repo.soft_delete(p.id).await.unwrap();
        assert!(repo.find_by_id(p.id).await.unwrap().is_none());
    }
}
