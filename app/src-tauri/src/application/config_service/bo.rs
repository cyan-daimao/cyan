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
    /// 软删时间（未删除为 None）
    pub deleted_at: Option<chrono::NaiveDateTime>,
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
            deleted_at: m.deleted_at,
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
    /// 软删时间（未删除为 None）
    pub deleted_at: Option<chrono::NaiveDateTime>,
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
            deleted_at: s.deleted_at,
        }
    }
}

/// MCP 市场条目 BO
#[derive(Debug, Clone)]
pub struct McpMarketItemBO {
    /// 服务器标识
    pub name: String,
    /// 展示标题
    pub title: String,
    /// 描述
    pub description: String,
    /// 版本
    pub version: String,
    /// 安装命令（None = 不可安装）
    pub command: Option<String>,
    /// 来源（featured/registry）
    pub source: String,
    /// 主页
    pub homepage: Option<String>,
}

impl From<crate::infra::mcp_registry::McpMarketItem> for McpMarketItemBO {
    fn from(m: crate::infra::mcp_registry::McpMarketItem) -> Self {
        Self {
            name: m.name,
            title: m.title,
            description: m.description,
            version: m.version,
            command: m.command,
            source: m.source.to_string(),
            homepage: m.homepage,
        }
    }
}

/// 权限规则 BO
#[derive(Debug, Clone)]
pub struct PermRuleBO {
    /// 规则 id
    pub id: i64,
    /// 作用域（global/project/session）
    pub scope: String,
    /// 所属项目 id（None = 非项目级）
    pub project_id: Option<i64>,
    /// 所属会话 id（None = 非会话级）
    pub session_id: Option<i64>,
    /// 工具名
    pub tool: String,
    /// glob 匹配模式
    pub pattern: String,
    /// 动作（allow/ask/deny）
    pub action: String,
    /// 匹配顺序
    pub sort: i64,
    /// 软删时间（未删除为 None）
    pub deleted_at: Option<chrono::NaiveDateTime>,
}

impl From<PermissionRule> for PermRuleBO {
    fn from(r: PermissionRule) -> Self {
        Self {
            id: r.id,
            scope: r.scope().as_str().to_string(),
            project_id: r.project_id,
            session_id: r.session_id,
            tool: r.tool,
            pattern: r.pattern,
            action: r.action.as_str().to_string(),
            sort: r.sort,
            deleted_at: r.deleted_at,
        }
    }
}
