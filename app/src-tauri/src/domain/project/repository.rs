//! 项目域 Repository trait（infra/db 实现）。

use async_trait::async_trait;

use super::Project;

/// 项目仓储
#[async_trait]
pub trait ProjectRepository: Send + Sync {
    /// 按 id 查询（过滤软删）
    async fn find_by_id(&self, id: i64) -> anyhow::Result<Option<Project>>;
    /// 按路径查询（过滤软删）
    async fn find_by_path(&self, path: &str) -> anyhow::Result<Option<Project>>;
    /// 最近项目列表（last_opened_at 倒序，NULL 排后）
    async fn list_recent(&self, limit: i64) -> anyhow::Result<Vec<Project>>;
    /// 插入并回填自增 id
    async fn insert(&self, project: &mut Project) -> anyhow::Result<()>;
    /// 更新最近打开时间
    async fn touch_last_opened(&self, id: i64) -> anyhow::Result<()>;
    /// 软删除（从最近项目移除，不删磁盘文件与会话记录）
    async fn soft_delete(&self, id: i64) -> anyhow::Result<()>;
}
