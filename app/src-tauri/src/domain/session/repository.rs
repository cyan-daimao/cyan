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
}

/// 消息仓储
#[async_trait]
pub trait MessageRepository: Send + Sync {
    /// 列出会话全部消息（seq 升序，过滤软删）
    async fn list_by_session(&self, session_id: i64) -> anyhow::Result<Vec<Message>>;
    /// 插入并回填自增 id
    async fn insert(&self, message: &mut Message) -> anyhow::Result<()>;
    /// 软删除会话全部消息
    async fn soft_delete_by_session(&self, session_id: i64) -> anyhow::Result<()>;
}
