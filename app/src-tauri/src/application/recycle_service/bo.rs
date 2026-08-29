//! 回收站业务对象（聚合六类软删记录）。

use crate::application::config_service::{McpServerBO, ModelBO, PermRuleBO};
use crate::application::plugin_service::PluginBO;
use crate::application::project_service::ProjectBO;
use crate::application::session_service::SessionBO;

/// 回收站聚合 BO
#[derive(Debug, Default)]
pub struct RecycleBinBO {
    /// 已删会话（带所属项目名称/路径）
    pub sessions: Vec<SessionBO>,
    /// 已删项目
    pub projects: Vec<ProjectBO>,
    /// 已删模型
    pub models: Vec<ModelBO>,
    /// 已删 MCP 服务器
    pub mcp_servers: Vec<McpServerBO>,
    /// 已删插件
    pub plugins: Vec<PluginBO>,
    /// 已删权限规则
    pub perm_rules: Vec<PermRuleBO>,
}
