//! 项目仓储：ProjectDO + Impl + From 转换。

use async_trait::async_trait;
use sqlx::{FromRow, SqlitePool};

use crate::domain::project::{Project, ProjectRepository};

use super::{fmt_time, now_local, parse_time, parse_time_opt};

/// 项目表行（cyan_project）
#[derive(Debug, FromRow)]
pub struct ProjectDO {
    /// 主键 id
    pub id: i64,
    /// 项目名
    pub name: String,
    /// 绝对路径（canonicalize 后）
    pub path: String,
    /// 最近打开时间
    pub last_opened_at: Option<String>,
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

impl TryFrom<ProjectDO> for Project {
    type Error = anyhow::Error;

    fn try_from(d: ProjectDO) -> anyhow::Result<Self> {
        Ok(Self {
            id: d.id,
            name: d.name,
            path: d.path,
            last_opened_at: parse_time_opt(&d.last_opened_at)?,
            created_at: parse_time(&d.created_at)?,
            updated_at: parse_time(&d.updated_at)?,
        })
    }
}

/// 项目仓储 SQLx 实现
pub struct ProjectRepositoryImpl {
    pool: SqlitePool,
}

impl ProjectRepositoryImpl {
    /// 构造
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    async fn find_one(&self, clause: &str, value: &str) -> anyhow::Result<Option<Project>> {
        let sql = format!(
            "SELECT id, name, path, last_opened_at, created_by, updated_by, created_at, updated_at, deleted_at
             FROM cyan_project WHERE {clause} = ? AND deleted_at IS NULL"
        );
        let row = sqlx::query_as::<_, ProjectDO>(&sql)
            .bind(value)
            .fetch_optional(&self.pool)
            .await?;
        row.map(Project::try_from).transpose()
    }
}

#[async_trait]
impl ProjectRepository for ProjectRepositoryImpl {
    async fn find_by_id(&self, id: i64) -> anyhow::Result<Option<Project>> {
        let row = sqlx::query_as::<_, ProjectDO>(
            "SELECT id, name, path, last_opened_at, created_by, updated_by, created_at, updated_at, deleted_at
             FROM cyan_project WHERE id = ? AND deleted_at IS NULL",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(Project::try_from).transpose()
    }

    async fn find_by_path(&self, path: &str) -> anyhow::Result<Option<Project>> {
        self.find_one("path", path).await
    }

    async fn list_recent(&self, limit: i64) -> anyhow::Result<Vec<Project>> {
        let rows = sqlx::query_as::<_, ProjectDO>(
            "SELECT id, name, path, last_opened_at, created_by, updated_by, created_at, updated_at, deleted_at
             FROM cyan_project WHERE deleted_at IS NULL
             ORDER BY last_opened_at IS NULL ASC, last_opened_at DESC
             LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(Project::try_from).collect()
    }

    async fn insert(&self, project: &mut Project) -> anyhow::Result<()> {
        let now = now_local();
        project.created_at = now;
        project.updated_at = now;
        let id = sqlx::query(
            "INSERT INTO cyan_project
                (name, path, last_opened_at, created_by, updated_by, created_at, updated_at)
             VALUES (?, ?, ?, 'local', 'local', ?, ?)",
        )
        .bind(&project.name)
        .bind(&project.path)
        .bind(project.last_opened_at.as_ref().map(fmt_time))
        .bind(fmt_time(&now))
        .bind(fmt_time(&now))
        .execute(&self.pool)
        .await?
        .last_insert_rowid();
        project.id = id;
        Ok(())
    }

    async fn touch_last_opened(&self, id: i64) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE cyan_project SET last_opened_at = ?, updated_by = 'local', updated_at = ?
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
