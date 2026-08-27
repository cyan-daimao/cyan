//! 配置业务对象。

use crate::domain::config::{McpServer, ModelConfig, PermissionRule};

/// 模型 BO（API Key 一律脱敏输出）
#[derive(Debug, Clone)]
pub struct ModelBO {
    /// 模型 id
    pub id: i64,
    /// 模型名
    pub name: String,
    /// Provider
    pub provider: String,
    /// Base URL
    pub base_url: String,
    /// 脱敏 API Key（`sk-****xxxx`）
    pub masked_key: String,
    /// 上下文窗口
    pub context_window: i64,
    /// 是否默认
    pub is_default: bool,
    /// 状态（enabled/disabled）
    pub status: String,
}

impl ModelBO {
    /// Domain → BO（附带脱敏 key）
    pub fn from_domain(m: ModelConfig, masked_key: String) -> Self {
        Self {
            id: m.id,
            name: m.name,
            provider: m.provider,
            base_url: m.base_url,
            masked_key,
            context_window: m.context_window,
            is_default: m.is_default,
            status: m.status.as_str().to_string(),
        }
    }
}

/// MCP 服务器 BO
#[derive(Debug, Clone)]
pub struct McpServerBO {
    /// 服务器 id
    pub id: i64,
    /// 服务器名
    pub name: String,
    /// 启动命令
    pub command: String,
    /// 状态（connected/error/disabled）
    pub status: String,
    /// 发现的工具数
    pub tools: i64,
    /// 最近失败原因
    pub last_error: Option<String>,
}

impl From<McpServer> for McpServerBO {
    fn from(s: McpServer) -> Self {
        Self {
            id: s.id,
            name: s.name,
            command: s.command,
            status: s.status.as_str().to_string(),
            tools: s.tools,
            last_error: s.last_error,
        }
    }
}

/// 权限规则 BO
#[derive(Debug, Clone)]
pub struct PermRuleBO {
    /// 规则 id
    pub id: i64,
    /// 工具名
    pub tool: String,
    /// glob 匹配模式
    pub pattern: String,
    /// 动作（allow/ask/deny）
    pub action: String,
    /// 匹配顺序
    pub sort: i64,
}

impl From<PermissionRule> for PermRuleBO {
    fn from(r: PermissionRule) -> Self {
        Self {
            id: r.id,
            tool: r.tool,
            pattern: r.pattern,
            action: r.action.as_str().to_string(),
            sort: r.sort,
        }
    }
}
