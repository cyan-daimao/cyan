//! checkpoint 仓储：CheckpointDO + Impl + From 转换。

use async_trait::async_trait;
use sqlx::{FromRow, SqlitePool};

use crate::domain::agent::{Checkpoint, CheckpointRepository};

use super::{fmt_time, now_local, parse_time};

/// checkpoint 表行（cyan_checkpoint）
#[derive(Debug, FromRow)]
pub struct CheckpointDO {
    /// 主键 id
    pub id: i64,
    /// 所属会话 id
    pub session_id: i64,
    /// 变更文件（相对项目）
    pub file_path: String,
    /// git blob 引用
    pub git_ref: String,
    /// 新增行数
    pub add_lines: i64,
    /// 删除行数
    pub del_lines: i64,
    /// 是否已回滚
    pub rolled_back: i64,
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

impl TryFrom<CheckpointDO> for Checkpoint {
    type Error = anyhow::Error;

    fn try_from(d: CheckpointDO) -> anyhow::Result<Self> {
        Ok(Self {
            id: d.id,
            session_id: d.session_id,
            file_path: d.file_path,
            git_ref: d.git_ref,
            add_lines: d.add_lines,
            del_lines: d.del_lines,
            rolled_back: d.rolled_back != 0,
            created_at: parse_time(&d.created_at)?,
            updated_at: parse_time(&d.updated_at)?,
        })
    }
}

const SELECT_COLS: &str =
    "id, session_id, file_path, git_ref, add_lines, del_lines, rolled_back,
     created_by, updated_by, created_at, updated_at, deleted_at";

/// checkpoint 仓储 SQLx 实现
pub struct CheckpointRepositoryImpl {
    pool: SqlitePool,
}

impl CheckpointRepositoryImpl {
    /// 构造
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CheckpointRepository for CheckpointRepositoryImpl {
    async fn insert(&self, checkpoint: &mut Checkpoint) -> anyhow::Result<()> {
        let now = now_local();
        checkpoint.created_at = now;
        checkpoint.updated_at = now;
        let id = sqlx::query(
            "INSERT INTO cyan_checkpoint
                (session_id, file_path, git_ref, add_lines, del_lines, rolled_back,
                 created_by, updated_by, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, 0, 'local', 'local', ?, ?)",
        )
        .bind(checkpoint.session_id)
        .bind(&checkpoint.file_path)
        .bind(&checkpoint.git_ref)
        .bind(checkpoint.add_lines)
        .bind(checkpoint.del_lines)
        .bind(fmt_time(&now))
        .bind(fmt_time(&now))
        .execute(&self.pool)
        .await?
        .last_insert_rowid();
        checkpoint.id = id;
        Ok(())
    }

    async fn find_by_id(&self, id: i64) -> anyhow::Result<Option<Checkpoint>> {
        let row = sqlx::query_as::<_, CheckpointDO>(&format!(
            "SELECT {SELECT_COLS} FROM cyan_checkpoint WHERE id = ? AND deleted_at IS NULL"
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(Checkpoint::try_from).transpose()
    }

    async fn list_by_session(&self, session_id: i64) -> anyhow::Result<Vec<Checkpoint>> {
        let rows = sqlx::query_as::<_, CheckpointDO>(&format!(
            "SELECT {SELECT_COLS} FROM cyan_checkpoint
             WHERE session_id = ? AND deleted_at IS NULL ORDER BY id ASC"
        ))
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(Checkpoint::try_from).collect()
    }

    async fn mark_rolled_back(&self, id: i64) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE cyan_checkpoint SET rolled_back = 1, updated_by = 'local', updated_at = ?
             WHERE id = ? AND deleted_at IS NULL",
        )
        .bind(fmt_time(&now_local()))
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
