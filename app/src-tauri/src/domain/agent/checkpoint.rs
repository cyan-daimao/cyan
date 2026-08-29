//! Checkpoint：变更快照（git blob 引用）与回滚端口。

use async_trait::async_trait;
use chrono::NaiveDateTime;

/// 变更 checkpoint
#[derive(Debug, Clone)]
pub struct Checkpoint {
    /// 主键 id（插入后回填）
    pub id: i64,
    /// 所属会话 id
    pub session_id: i64,
    /// 变更文件（相对项目）
    pub file_path: String,
    /// git blob 引用（变更前内容）
    pub git_ref: String,
    /// 新增行数
    pub add_lines: i64,
    /// 删除行数
    pub del_lines: i64,
    /// 是否已回滚
    pub rolled_back: bool,
    /// 创建时间
    pub created_at: NaiveDateTime,
    /// 更新时间
    pub updated_at: NaiveDateTime,
}

impl Checkpoint {
    /// 新建（未持久化，id 待回填）
    pub fn new(
        session_id: i64,
        file_path: String,
        git_ref: String,
        add_lines: i64,
        del_lines: i64,
        now: NaiveDateTime,
    ) -> Self {
        Self {
            id: 0,
            session_id,
            file_path,
            git_ref,
            add_lines,
            del_lines,
            rolled_back: false,
            created_at: now,
            updated_at: now,
        }
    }
}

/// checkpoint 仓储
#[async_trait]
pub trait CheckpointRepository: Send + Sync {
    /// 插入并回填自增 id
    async fn insert(&self, checkpoint: &mut Checkpoint) -> anyhow::Result<()>;
    /// 按 id 查询（过滤软删）
    async fn find_by_id(&self, id: i64) -> anyhow::Result<Option<Checkpoint>>;
    /// 列出会话变更（过滤软删）
    async fn list_by_session(&self, session_id: i64) -> anyhow::Result<Vec<Checkpoint>>;
    /// 标记已回滚
    async fn mark_rolled_back(&self, id: i64) -> anyhow::Result<()>;
    /// 软删会话全部 checkpoint（项目级联移除用）
    async fn soft_delete_by_session(&self, session_id: i64) -> anyhow::Result<()>;
    /// 窗口级联软删：项目下所有会话的 checkpoint（统一 deleted_at 时间戳）
    async fn soft_delete_by_project_window(&self, project_id: i64, deleted_at: &str) -> anyhow::Result<()>;
    /// 窗口级联恢复：仅还原 deleted_at == 窗口时间戳的 checkpoint
    async fn restore_by_project_window(&self, project_id: i64, deleted_at: &str) -> anyhow::Result<()>;
}

/// checkpoint 回滚端口（infra/git 实现）
pub trait CheckpointGateway: Send + Sync {
    /// 将 git_ref 指向的变更前内容写回工作区文件
    fn rollback(&self, project_root: &std::path::Path, git_ref: &str, rel_path: &str) -> anyhow::Result<()>;
}
