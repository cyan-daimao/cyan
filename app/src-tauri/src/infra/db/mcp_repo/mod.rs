//! MCP 服务器仓储：McpServerDO + Impl + From 转换。

use async_trait::async_trait;
use sqlx::{FromRow, SqlitePool};

use crate::domain::config::{McpRepository, McpServer, McpStatus};

use super::{fmt_time, now_local, parse_time};

/// MCP 服务器表行（cyan_mcp_server）
#[derive(Debug, FromRow)]
pub struct McpServerDO {
    /// 主键 id
    pub id: i64,
    /// 服务器名（唯一）
    pub name: String,
    /// 启动命令
    pub command: String,
    /// 状态（connected/error/disabled）
    pub status: String,
    /// 握手发现的工具数
    pub tools: i64,
    /// 最近失败原因
    pub last_error: Option<String>,
    /// 来源插件名（NULL = 用户自建）
    pub plugin_origin: Option<String>,
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

impl TryFrom<McpServerDO> for McpServer {
    type Error = anyhow::Error;

    fn try_from(d: McpServerDO) -> anyhow::Result<Self> {
        Ok(Self {
            id: d.id,
            name: d.name,
            command: d.command,
            status: McpStatus::parse(&d.status),
            tools: d.tools,
            last_error: d.last_error,
            plugin_origin: d.plugin_origin,
            created_at: parse_time(&d.created_at)?,
            updated_at: parse_time(&d.updated_at)?,
        })
    }
}

const SELECT_COLS: &str =
    "id, name, command, status, tools, last_error, plugin_origin, created_by, updated_by, created_at, updated_at, deleted_at";

/// MCP 服务器仓储 SQLx 实现
pub struct McpRepositoryImpl {
    pool: SqlitePool,
}

impl McpRepositoryImpl {
    /// 构造
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl McpRepository for McpRepositoryImpl {
    async fn list(&self) -> anyhow::Result<Vec<McpServer>> {
        let rows = sqlx::query_as::<_, McpServerDO>(&format!(
            "SELECT {SELECT_COLS} FROM cyan_mcp_server WHERE deleted_at IS NULL ORDER BY id ASC"
        ))
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(McpServer::try_from).collect()
    }

    async fn find_by_name(&self, name: &str) -> anyhow::Result<Option<McpServer>> {
        let row = sqlx::query_as::<_, McpServerDO>(&format!(
            "SELECT {SELECT_COLS} FROM cyan_mcp_server WHERE name = ? AND deleted_at IS NULL"
        ))
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;
        row.map(McpServer::try_from).transpose()
    }

    async fn insert(&self, server: &mut McpServer) -> anyhow::Result<()> {
        let now = now_local();
        server.created_at = now;
        server.updated_at = now;
        let id = sqlx::query(
            "INSERT INTO cyan_mcp_server
                (name, command, status, tools, last_error, plugin_origin, created_by, updated_by, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, 'local', 'local', ?, ?)",
        )
        .bind(&server.name)
        .bind(&server.command)
        .bind(server.status.as_str())
        .bind(server.tools)
        .bind(&server.last_error)
        .bind(&server.plugin_origin)
        .bind(fmt_time(&now))
        .bind(fmt_time(&now))
        .execute(&self.pool)
        .await?
        .last_insert_rowid();
        server.id = id;
        Ok(())
    }

    async fn update(&self, server: &McpServer) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE cyan_mcp_server
             SET command = ?, status = ?, tools = ?, last_error = ?,
                 updated_by = 'local', updated_at = ?
             WHERE id = ? AND deleted_at IS NULL",
        )
        .bind(&server.command)
        .bind(server.status.as_str())
        .bind(server.tools)
        .bind(&server.last_error)
        .bind(fmt_time(&now_local()))
        .bind(server.id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn soft_delete(&self, id: i64) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE cyan_mcp_server SET deleted_at = ?, updated_by = 'local', updated_at = ?
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
