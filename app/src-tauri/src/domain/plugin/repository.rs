//! 插件域 Repository trait（infra/db 实现）。

use async_trait::async_trait;

use super::Plugin;

/// 插件仓储
#[async_trait]
pub trait PluginRepository: Send + Sync {
    /// 全量列表（过滤软删，按安装时间倒序）
    async fn list(&self) -> anyhow::Result<Vec<Plugin>>;
    /// 按 id 查询
    async fn find_by_id(&self, id: i64) -> anyhow::Result<Option<Plugin>>;
    /// 按名称查询
    async fn find_by_name(&self, name: &str) -> anyhow::Result<Option<Plugin>>;
    /// 插入并回填自增 id
    async fn insert(&self, plugin: &mut Plugin) -> anyhow::Result<()>;
    /// 更新（状态/计数）
    async fn update(&self, plugin: &Plugin) -> anyhow::Result<()>;
    /// 软删除
    async fn soft_delete(&self, id: i64) -> anyhow::Result<()>;
    /// 回收站：软删插件列表（deleted_at 非空）
    async fn list_deleted(&self) -> anyhow::Result<Vec<Plugin>>;
    /// 恢复软删插件（清 deleted_at，幂等；保持 disabled 待用户手动启用）
    async fn restore(&self, id: i64) -> anyhow::Result<()>;
}
