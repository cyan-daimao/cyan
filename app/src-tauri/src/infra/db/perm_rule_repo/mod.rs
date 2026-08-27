//! 权限规则仓储：PermRuleDO + Impl + From 转换。

use async_trait::async_trait;
use sqlx::{FromRow, SqlitePool};

use crate::domain::config::{PermAction, PermRuleRepository, PermissionRule};
use crate::domain::DomainError;

use super::{fmt_time, now_local, parse_time};

/// 权限规则表行（cyan_permission_rule）
#[derive(Debug, FromRow)]
pub struct PermRuleDO {
    /// 主键 id
    pub id: i64,
    /// 工具名
    pub tool: String,
    /// glob 匹配模式
    pub pattern: String,
    /// 动作（allow/ask/deny）
    pub action: String,
    /// 匹配顺序
    pub sort: i64,
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

impl TryFrom<PermRuleDO> for PermissionRule {
    type Error = anyhow::Error;

    fn try_from(d: PermRuleDO) -> anyhow::Result<Self> {
        Ok(Self {
            id: d.id,
            tool: d.tool,
            pattern: d.pattern,
            action: PermAction::parse(&d.action).ok_or_else(|| {
                DomainError::Validation(format!("未知权限动作：{}", d.action))
            })?,
            sort: d.sort,
            created_at: parse_time(&d.created_at)?,
            updated_at: parse_time(&d.updated_at)?,
        })
    }
}

const SELECT_COLS: &str =
    "id, tool, pattern, action, sort, created_by, updated_by, created_at, updated_at, deleted_at";

/// 权限规则仓储 SQLx 实现
pub struct PermRuleRepositoryImpl {
    pool: SqlitePool,
}

impl PermRuleRepositoryImpl {
    /// 构造
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PermRuleRepository for PermRuleRepositoryImpl {
    async fn list(&self) -> anyhow::Result<Vec<PermissionRule>> {
        let rows = sqlx::query_as::<_, PermRuleDO>(&format!(
            "SELECT {SELECT_COLS} FROM cyan_permission_rule
             WHERE deleted_at IS NULL ORDER BY sort ASC, id ASC"
        ))
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(PermissionRule::try_from).collect()
    }

    async fn find_by_tool_pattern(
        &self,
        tool: &str,
        pattern: &str,
    ) -> anyhow::Result<Option<PermissionRule>> {
        let row = sqlx::query_as::<_, PermRuleDO>(&format!(
            "SELECT {SELECT_COLS} FROM cyan_permission_rule
             WHERE tool = ? AND pattern = ? AND deleted_at IS NULL"
        ))
        .bind(tool)
        .bind(pattern)
        .fetch_optional(&self.pool)
        .await?;
        row.map(PermissionRule::try_from).transpose()
    }

    async fn insert(&self, rule: &mut PermissionRule) -> anyhow::Result<()> {
        let now = now_local();
        rule.created_at = now;
        rule.updated_at = now;
        let id = sqlx::query(
            "INSERT INTO cyan_permission_rule
                (tool, pattern, action, sort, created_by, updated_by, created_at, updated_at)
             VALUES (?, ?, ?, ?, 'local', 'local', ?, ?)",
        )
        .bind(&rule.tool)
        .bind(&rule.pattern)
        .bind(rule.action.as_str())
        .bind(rule.sort)
        .bind(fmt_time(&now))
        .bind(fmt_time(&now))
        .execute(&self.pool)
        .await?
        .last_insert_rowid();
        rule.id = id;
        Ok(())
    }

    async fn update(&self, rule: &PermissionRule) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE cyan_permission_rule
             SET action = ?, sort = ?, updated_by = 'local', updated_at = ?
             WHERE id = ? AND deleted_at IS NULL",
        )
        .bind(rule.action.as_str())
        .bind(rule.sort)
        .bind(fmt_time(&now_local()))
        .bind(rule.id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn soft_delete(&self, id: i64) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE cyan_permission_rule SET deleted_at = ?, updated_by = 'local', updated_at = ?
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
