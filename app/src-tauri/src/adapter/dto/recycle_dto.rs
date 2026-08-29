//! 回收站相关 Request / DTO。

use serde::{Deserialize, Serialize};

use crate::adapter::dto::{
    McpServerDTO, ModelDTO, PermRuleDTO, PluginDTO, SessionDTO,
};
use crate::application::recycle_service::{RecycleBinBO, RestoreRecycleItemCmd};
use crate::application::project_service::ProjectBO;
use crate::infra::db::fmt_time;

/// restore_recycle_item 请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreRecycleItemRequest {
    /// 对象类别（session/project/model/mcp/plugin/permRule）
    pub kind: String,
    /// 对象 id
    pub id: i64,
}

impl From<RestoreRecycleItemRequest> for RestoreRecycleItemCmd {
    fn from(r: RestoreRecycleItemRequest) -> Self {
        Self {
            kind: r.kind,
            id: r.id,
        }
    }
}

/// 回收站项目 DTO（ProjectDTO + deletedAt）
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRecycleDTO {
    /// 项目 id
    pub id: i64,
    /// 项目名
    pub name: String,
    /// 绝对路径
    pub path: String,
    /// 最近打开时间
    pub last_opened_at: Option<String>,
    /// 删除时间
    pub deleted_at: Option<String>,
}

impl From<ProjectBO> for ProjectRecycleDTO {
    fn from(bo: ProjectBO) -> Self {
        Self {
            id: bo.id,
            name: bo.name,
            path: bo.path,
            last_opened_at: bo.last_opened_at.as_ref().map(fmt_time),
            deleted_at: bo.deleted_at.as_ref().map(fmt_time),
        }
    }
}

/// 回收站聚合 DTO
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecycleBinDTO {
    /// 已删会话
    pub sessions: Vec<SessionDTO>,
    /// 已删项目
    pub projects: Vec<ProjectRecycleDTO>,
    /// 已删模型
    pub models: Vec<ModelDTO>,
    /// 已删 MCP 服务器
    pub mcp_servers: Vec<McpServerDTO>,
    /// 已删插件
    pub plugins: Vec<PluginDTO>,
    /// 已删权限规则
    pub perm_rules: Vec<PermRuleDTO>,
}

impl From<RecycleBinBO> for RecycleBinDTO {
    fn from(bo: RecycleBinBO) -> Self {
        Self {
            sessions: bo.sessions.into_iter().map(SessionDTO::from).collect(),
            projects: bo.projects.into_iter().map(ProjectRecycleDTO::from).collect(),
            models: bo.models.into_iter().map(ModelDTO::from).collect(),
            mcp_servers: bo.mcp_servers.into_iter().map(McpServerDTO::from).collect(),
            plugins: bo.plugins.into_iter().map(PluginDTO::from).collect(),
            perm_rules: bo.perm_rules.into_iter().map(PermRuleDTO::from).collect(),
        }
    }
}
