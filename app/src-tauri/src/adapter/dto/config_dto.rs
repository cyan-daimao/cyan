//! 配置相关 Request / DTO（模型 / MCP / 权限规则）。

use serde::{Deserialize, Serialize};

use crate::application::config_service::{
    DeleteMcpCmd, DeleteModelCmd, DeletePermRuleCmd, McpServerBO, ModelBO, PermRuleBO, SaveMcpCmd,
    SaveModelCmd, SavePermRuleCmd, SetDefaultModelCmd, ToggleMcpCmd,
};

/// save_model 请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveModelRequest {
    /// 模型 id（编辑时携带）
    pub id: Option<i64>,
    /// 模型名
    pub name: String,
    /// Provider
    pub provider: String,
    /// Base URL
    pub base_url: String,
    /// API Key（空/缺省表示不修改）
    pub api_key: Option<String>,
    /// 上下文窗口
    pub context_window: i64,
    /// 是否启用
    pub enabled: bool,
}

impl From<SaveModelRequest> for SaveModelCmd {
    fn from(r: SaveModelRequest) -> Self {
        Self {
            id: r.id,
            name: r.name,
            provider: r.provider,
            base_url: r.base_url,
            api_key: r.api_key,
            context_window: r.context_window,
            enabled: r.enabled,
        }
    }
}

/// delete_model 请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteModelRequest {
    /// 模型 id
    pub id: i64,
}

impl From<DeleteModelRequest> for DeleteModelCmd {
    fn from(r: DeleteModelRequest) -> Self {
        Self { id: r.id }
    }
}

/// set_default_model 请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetDefaultModelRequest {
    /// 模型 id
    pub id: i64,
}

impl From<SetDefaultModelRequest> for SetDefaultModelCmd {
    fn from(r: SetDefaultModelRequest) -> Self {
        Self { id: r.id }
    }
}

/// 模型 DTO（API Key 已脱敏）
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDTO {
    /// 模型 id
    pub id: i64,
    /// 模型名
    pub name: String,
    /// Provider
    pub provider: String,
    /// Base URL
    pub base_url: String,
    /// 脱敏 API Key
    pub masked_key: String,
    /// 上下文窗口
    pub context_window: i64,
    /// 是否默认
    pub is_default: bool,
    /// 状态
    pub status: String,
}

impl From<ModelBO> for ModelDTO {
    fn from(bo: ModelBO) -> Self {
        Self {
            id: bo.id,
            name: bo.name,
            provider: bo.provider,
            base_url: bo.base_url,
            masked_key: bo.masked_key,
            context_window: bo.context_window,
            is_default: bo.is_default,
            status: bo.status,
        }
    }
}

/// save_mcp_server 请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveMcpRequest {
    /// 服务器 id（编辑时携带）
    pub id: Option<i64>,
    /// 服务器名
    pub name: String,
    /// 启动命令
    pub command: String,
}

impl From<SaveMcpRequest> for SaveMcpCmd {
    fn from(r: SaveMcpRequest) -> Self {
        Self {
            id: r.id,
            name: r.name,
            command: r.command,
        }
    }
}

/// toggle_mcp_server 请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToggleMcpRequest {
    /// 服务器 id
    pub id: i64,
    /// 启用/禁用
    pub enable: bool,
}

impl From<ToggleMcpRequest> for ToggleMcpCmd {
    fn from(r: ToggleMcpRequest) -> Self {
        Self {
            id: r.id,
            enable: r.enable,
        }
    }
}

/// delete_mcp_server 请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteMcpRequest {
    /// 服务器 id
    pub id: i64,
}

impl From<DeleteMcpRequest> for DeleteMcpCmd {
    fn from(r: DeleteMcpRequest) -> Self {
        Self { id: r.id }
    }
}

/// MCP 服务器 DTO
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerDTO {
    /// 服务器 id
    pub id: i64,
    /// 服务器名
    pub name: String,
    /// 启动命令
    pub command: String,
    /// 状态
    pub status: String,
    /// 工具数
    pub tools: i64,
    /// 最近失败原因
    pub last_error: Option<String>,
}

impl From<McpServerBO> for McpServerDTO {
    fn from(bo: McpServerBO) -> Self {
        Self {
            id: bo.id,
            name: bo.name,
            command: bo.command,
            status: bo.status,
            tools: bo.tools,
            last_error: bo.last_error,
        }
    }
}

/// save_perm_rule 请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavePermRuleRequest {
    /// 规则 id（编辑时携带）
    pub id: Option<i64>,
    /// 工具名
    pub tool: String,
    /// glob 匹配模式
    pub pattern: String,
    /// 动作（allow/ask/deny）
    pub action: String,
    /// 匹配顺序
    pub sort: i64,
}

impl From<SavePermRuleRequest> for SavePermRuleCmd {
    fn from(r: SavePermRuleRequest) -> Self {
        Self {
            id: r.id,
            tool: r.tool,
            pattern: r.pattern,
            action: r.action,
            sort: r.sort,
        }
    }
}

/// delete_perm_rule 请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletePermRuleRequest {
    /// 规则 id
    pub id: i64,
}

impl From<DeletePermRuleRequest> for DeletePermRuleCmd {
    fn from(r: DeletePermRuleRequest) -> Self {
        Self { id: r.id }
    }
}

/// 权限规则 DTO
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermRuleDTO {
    /// 规则 id
    pub id: i64,
    /// 工具名
    pub tool: String,
    /// glob 匹配模式
    pub pattern: String,
    /// 动作
    pub action: String,
    /// 匹配顺序
    pub sort: i64,
}

impl From<PermRuleBO> for PermRuleDTO {
    fn from(bo: PermRuleBO) -> Self {
        Self {
            id: bo.id,
            tool: bo.tool,
            pattern: bo.pattern,
            action: bo.action,
            sort: bo.sort,
        }
    }
}
