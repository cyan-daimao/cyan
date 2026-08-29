//! 会话域 Repository trait（infra/db 实现）。

use async_trait::async_trait;

use super::{Message, Session};

/// 会话仓储
#[async_trait]
pub trait SessionRepository: Send + Sync {
    /// 按 id 查询（不含消息，过滤软删）
    async fn find_by_id(&self, id: i64) -> anyhow::Result<Option<Session>>;
    /// 按项目列出会话（updated_at 倒序，可按关键字过滤标题）
    async fn list_by_project(
        &self,
        project_id: i64,
        keyword: Option<&str>,
    ) -> anyhow::Result<Vec<Session>>;
    /// 插入并回填自增 id
    async fn insert(&self, session: &mut Session) -> anyhow::Result<()>;
    /// 更新（标题/ctx/token 统计）
    async fn update(&self, session: &Session) -> anyhow::Result<()>;
    /// 软删除
    async fn soft_delete(&self, id: i64) -> anyhow::Result<()>;
    /// 按项目聚合 token 用量（输入、输出、会话数，过滤软删）
    async fn sum_tokens_by_project(&self, project_id: i64) -> anyhow::Result<(i64, i64, i64)>;
    /// 回收站：软删会话列表（deleted_at 非空，按删除时间倒序）
    async fn list_deleted(&self) -> anyhow::Result<Vec<Session>>;
    /// 恢复软删会话（清 deleted_at，幂等）
    async fn restore(&self, id: i64) -> anyhow::Result<()>;
}

/// 消息仓储
#[async_trait]
pub trait MessageRepository: Send + Sync {
    /// 列出会话全部消息（seq 升序，过滤软删）
    async fn list_by_session(&self, session_id: i64) -> anyhow::Result<Vec<Message>>;
    /// 插入并回填自增 id
    async fn insert(&self, message: &mut Message) -> anyhow::Result<()>;
    /// 更新消息载荷（审批 pending → 最终决断）
    async fn update_payload(&self, id: i64, payload: &str) -> anyhow::Result<()>;
    /// 软删除会话全部消息
    async fn soft_delete_by_session(&self, session_id: i64) -> anyhow::Result<()>;
    /// 恢复会话全部软删消息（幂等）
    async fn restore_by_session(&self, session_id: i64) -> anyhow::Result<()>;
}

/// 回收站仓储（跨表维护：全库软删记录硬删）
#[async_trait]
pub trait RecycleBinRepository: Send + Sync {
    /// 硬删全部 8 张表的软删记录（单事务，FK 顺序），返回总删除行数
    async fn purge_soft_deleted(&self) -> anyhow::Result<i64>;
}
