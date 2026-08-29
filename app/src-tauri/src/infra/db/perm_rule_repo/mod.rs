//! 权限规则仓储：PermRuleDO + Impl + From 转换。

use async_trait::async_trait;
use sqlx::{FromRow, SqlitePool};

use crate::domain::config::{PermAction, PermRuleRepository, PermissionRule};
use crate::domain::DomainError;

use super::{fmt_time, now_local, parse_time, parse_time_opt};

/// 权限规则表行（cyan_permission_rule）
#[derive(Debug, FromRow)]
pub struct PermRuleDO {
    /// 主键 id
    pub id: i64,
    /// 所属项目 id（NULL = 非项目级）
    pub project_id: Option<i64>,
    /// 所属会话 id（NULL = 非会话级）
    pub session_id: Option<i64>,
    /// 工具名
    pub tool: String,
    /// glob 匹配模式
    pub pattern: String,
    /// 动作（allow/ask/deny）
    pub action: String,
    /// 匹配顺序
    pub sort: i64,
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

impl TryFrom<PermRuleDO> for PermissionRule {
    type Error = anyhow::Error;

    fn try_from(d: PermRuleDO) -> anyhow::Result<Self> {
        Ok(Self {
            id: d.id,
            project_id: d.project_id,
            session_id: d.session_id,
            tool: d.tool,
            pattern: d.pattern,
            action: PermAction::parse(&d.action).ok_or_else(|| {
                DomainError::Validation(format!("未知权限动作：{}", d.action))
            })?,
            sort: d.sort,
            plugin_origin: d.plugin_origin,
            created_at: parse_time(&d.created_at)?,
            updated_at: parse_time(&d.updated_at)?,
            deleted_at: parse_time_opt(&d.deleted_at)?,
        })
    }
}

const SELECT_COLS: &str =
    "id, project_id, session_id, tool, pattern, action, sort, plugin_origin, created_by, updated_by, created_at, updated_at, deleted_at";

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
    async fn list_global(&self) -> anyhow::Result<Vec<PermissionRule>> {
        let rows = sqlx::query_as::<_, PermRuleDO>(&format!(
            "SELECT {SELECT_COLS} FROM cyan_permission_rule
             WHERE deleted_at IS NULL AND project_id IS NULL AND session_id IS NULL
             ORDER BY sort ASC, id ASC"
        ))
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(PermissionRule::try_from).collect()
    }

    async fn list_visible(
        &self,
        session_id: i64,
        project_id: i64,
    ) -> anyhow::Result<Vec<PermissionRule>> {
        let rows = sqlx::query_as::<_, PermRuleDO>(&format!(
            "SELECT {SELECT_COLS} FROM cyan_permission_rule
             WHERE deleted_at IS NULL
               AND ((project_id IS NULL AND session_id IS NULL)
                    OR (session_id IS NULL AND project_id = ?)
                    OR session_id = ?)
             ORDER BY sort ASC, id ASC"
        ))
        .bind(project_id)
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(PermissionRule::try_from).collect()
    }

    async fn find_by_tool_pattern(
        &self,
        tool: &str,
        pattern: &str,
        project_id: Option<i64>,
        session_id: Option<i64>,
    ) -> anyhow::Result<Option<PermissionRule>> {
        let sql = format!(
            "SELECT {SELECT_COLS} FROM cyan_permission_rule
             WHERE tool = ? AND pattern = ? AND deleted_at IS NULL
               AND {} AND {}",
            if project_id.is_some() { "project_id = ?" } else { "project_id IS NULL" },
            if session_id.is_some() { "session_id = ?" } else { "session_id IS NULL" }
        );
        let mut q = sqlx::query_as::<_, PermRuleDO>(&sql).bind(tool).bind(pattern);
        if let Some(pid) = project_id {
            q = q.bind(pid);
        }
        if let Some(sid) = session_id {
            q = q.bind(sid);
        }
        let row = q.fetch_optional(&self.pool).await?;
        row.map(PermissionRule::try_from).transpose()
    }

    async fn find_by_id(&self, id: i64) -> anyhow::Result<Option<PermissionRule>> {
        let row = sqlx::query_as::<_, PermRuleDO>(&format!(
            "SELECT {SELECT_COLS} FROM cyan_permission_rule
             WHERE id = ? AND deleted_at IS NULL"
        ))
        .bind(id)
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
                (project_id, session_id, tool, pattern, action, sort, plugin_origin, created_by, updated_by, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, 'local', 'local', ?, ?)",
        )
        .bind(rule.project_id)
        .bind(rule.session_id)
        .bind(&rule.tool)
        .bind(&rule.pattern)
        .bind(rule.action.as_str())
        .bind(rule.sort)
        .bind(&rule.plugin_origin)
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

    async fn soft_delete_by_plugin_origin(&self, origin: &str) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE cyan_permission_rule SET deleted_at = ?, updated_by = 'local', updated_at = ?
             WHERE plugin_origin = ? AND deleted_at IS NULL",
        )
        .bind(fmt_time(&now_local()))
        .bind(fmt_time(&now_local()))
        .bind(origin)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn soft_delete_by_project(&self, project_id: i64) -> anyhow::Result<()> {
        // 仅软删项目级规则（project_id = X，session_id IS NULL），不动全局与本项目内 session 级规则
        sqlx::query(
            "UPDATE cyan_permission_rule SET deleted_at = ?, updated_by = 'local', updated_at = ?
             WHERE project_id = ? AND session_id IS NULL AND deleted_at IS NULL",
        )
        .bind(fmt_time(&now_local()))
        .bind(fmt_time(&now_local()))
        .bind(project_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn soft_delete_by_project_window(&self, project_id: i64, deleted_at: &str) -> anyhow::Result<()> {
        // 窗口级联：仅项目级规则（session_id IS NULL），用统一时间戳便于同窗恢复
        sqlx::query(
            "UPDATE cyan_permission_rule SET deleted_at = ?, updated_by = 'local', updated_at = ?
             WHERE project_id = ? AND session_id IS NULL AND deleted_at IS NULL",
        )
        .bind(deleted_at)
        .bind(fmt_time(&now_local()))
        .bind(project_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn restore_project_rules_window(&self, project_id: i64, deleted_at: &str) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE cyan_permission_rule SET deleted_at = NULL, updated_by = 'local', updated_at = ?
             WHERE project_id = ? AND session_id IS NULL AND deleted_at = ?",
        )
        .bind(fmt_time(&now_local()))
        .bind(project_id)
        .bind(deleted_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_deleted(&self) -> anyhow::Result<Vec<PermissionRule>> {
        let rows = sqlx::query_as::<_, PermRuleDO>(&format!(
            "SELECT {SELECT_COLS} FROM cyan_permission_rule
             WHERE deleted_at IS NOT NULL ORDER BY deleted_at DESC, sort ASC"
        ))
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(PermissionRule::try_from).collect()
    }

    async fn restore(&self, id: i64) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE cyan_permission_rule SET deleted_at = NULL, updated_by = 'local', updated_at = ?
             WHERE id = ? AND deleted_at IS NOT NULL",
        )
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

    async fn seed_sessions(pool: &SqlitePool) -> (i64, i64, i64) {
        let pid = sqlx::query(
            "INSERT INTO cyan_project (name, path, created_by, updated_by, created_at, updated_at)
             VALUES ('demo', '/tmp/demo', 'local', 'local', '2026-08-27 10:00:00', '2026-08-27 10:00:00')",
        )
        .execute(pool)
        .await
        .unwrap()
        .last_insert_rowid();
        let mut ids = Vec::new();
        for title in ["s1", "s2"] {
            let sid = sqlx::query(
                "INSERT INTO cyan_session (project_id, title, created_by, updated_by, created_at, updated_at)
                 VALUES (?, ?, 'local', 'local', '2026-08-27 10:00:00', '2026-08-27 10:00:00')",
            )
            .bind(pid)
            .bind(title)
            .execute(pool)
            .await
            .unwrap()
            .last_insert_rowid();
            ids.push(sid);
        }
        (pid, ids[0], ids[1])
    }

    fn rule(project_id: Option<i64>, session_id: Option<i64>, tool: &str, pattern: &str) -> PermissionRule {
        PermissionRule {
            id: 0,
            project_id,
            session_id,
            tool: tool.into(),
            pattern: pattern.into(),
            action: PermAction::Allow,
            sort: 0,
            plugin_origin: None,
            created_at: now_local(),
            updated_at: now_local(),
            deleted_at: None,
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn list_visible_returns_global_project_and_own_session_only(pool: SqlitePool) {
        let (pid, sid, sid2) = seed_sessions(&pool).await;
        let repo = PermRuleRepositoryImpl::new(pool.clone());

        let mut global = rule(None, None, "Bash", "cargo *");
        repo.insert(&mut global).await.unwrap();
        let mut proj = rule(Some(pid), None, "Edit", "src/**");
        repo.insert(&mut proj).await.unwrap();
        let mut mine = rule(Some(pid), Some(sid), "Write", "a/**");
        repo.insert(&mut mine).await.unwrap();
        let mut other = rule(Some(pid), Some(sid2), "Write", "b/**");
        repo.insert(&mut other).await.unwrap();

        let visible = repo.list_visible(sid, pid).await.unwrap();
        assert_eq!(visible.len(), 3, "应只见全局 + 本项目 + 本会话规则");
        assert!(visible.iter().any(|r| r.scope() == crate::domain::config::RuleScope::Global));
        assert!(visible.iter().any(|r| r.scope() == crate::domain::config::RuleScope::Project));
        assert!(visible.iter().any(|r| r.session_id == Some(sid)));

        // 全局列表只含双 NULL 规则
        let globals = repo.list_global().await.unwrap();
        assert_eq!(globals.len(), 1);

        // 同 tool+pattern 在不同作用域各存一条
        let dup = repo
            .find_by_tool_pattern("Write", "a/**", Some(pid), Some(sid2))
            .await
            .unwrap();
        assert!(dup.is_none());
        let hit = repo
            .find_by_tool_pattern("Write", "a/**", Some(pid), Some(sid))
            .await
            .unwrap();
        assert!(hit.is_some());
    }
}
