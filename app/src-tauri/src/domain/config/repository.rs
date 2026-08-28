//! 配置域 Repository trait（infra/db 实现）。

use async_trait::async_trait;

use super::{McpServer, ModelConfig, PermissionRule};

/// 模型配置仓储
#[async_trait]
pub trait ModelRepository: Send + Sync {
    /// 全量列表（过滤软删）
    async fn list(&self) -> anyhow::Result<Vec<ModelConfig>>;
    /// 按 id 查询
    async fn find_by_id(&self, id: i64) -> anyhow::Result<Option<ModelConfig>>;
    /// 按名称查询
    async fn find_by_name(&self, name: &str) -> anyhow::Result<Option<ModelConfig>>;
    /// 查询默认模型
    async fn find_default(&self) -> anyhow::Result<Option<ModelConfig>>;
    /// 插入并回填自增 id
    async fn insert(&self, model: &mut ModelConfig) -> anyhow::Result<()>;
    /// 更新
    async fn update(&self, model: &ModelConfig) -> anyhow::Result<()>;
    /// 软删除
    async fn soft_delete(&self, id: i64) -> anyhow::Result<()>;
    /// 清除全部默认标记
    async fn clear_default(&self) -> anyhow::Result<()>;
}

/// MCP 服务器仓储
#[async_trait]
pub trait McpRepository: Send + Sync {
    /// 全量列表（过滤软删）
    async fn list(&self) -> anyhow::Result<Vec<McpServer>>;
    /// 按名称查询
    async fn find_by_name(&self, name: &str) -> anyhow::Result<Option<McpServer>>;
    /// 插入并回填自增 id
    async fn insert(&self, server: &mut McpServer) -> anyhow::Result<()>;
    /// 更新
    async fn update(&self, server: &McpServer) -> anyhow::Result<()>;
    /// 软删除
    async fn soft_delete(&self, id: i64) -> anyhow::Result<()>;
}

/// 权限规则仓储
#[async_trait]
pub trait PermRuleRepository: Send + Sync {
    /// 全局规则列表（双 NULL，sort 升序，过滤软删）
    async fn list_global(&self) -> anyhow::Result<Vec<PermissionRule>>;
    /// 会话可见规则列表：全局 + 该项目 + 该会话（sort 升序，过滤软删）
    async fn list_visible(
        &self,
        session_id: i64,
        project_id: i64,
    ) -> anyhow::Result<Vec<PermissionRule>>;
    /// 按 (tool, pattern, project_id, session_id) 精确作用域查询
    async fn find_by_tool_pattern(
        &self,
        tool: &str,
        pattern: &str,
        project_id: Option<i64>,
        session_id: Option<i64>,
    ) -> anyhow::Result<Option<PermissionRule>>;
    /// 按 id 查询（过滤软删）
    async fn find_by_id(&self, id: i64) -> anyhow::Result<Option<PermissionRule>>;
    /// 插入并回填自增 id
    async fn insert(&self, rule: &mut PermissionRule) -> anyhow::Result<()>;
    /// 更新
    async fn update(&self, rule: &PermissionRule) -> anyhow::Result<()>;
    /// 软删除
    async fn soft_delete(&self, id: i64) -> anyhow::Result<()>;
    /// 按来源插件软删除（插件禁用/卸载时摘除规则）
    async fn soft_delete_by_plugin_origin(&self, origin: &str) -> anyhow::Result<()>;
}
