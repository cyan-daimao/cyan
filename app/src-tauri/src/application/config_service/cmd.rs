//! 配置命令对象。

/// 保存模型命令（按 name 幂等 upsert）
#[derive(Debug, Clone)]
pub struct SaveModelCmd {
    /// 模型 id（编辑时携带，新建为 None）
    pub id: Option<i64>,
    /// 模型名（唯一）
    pub name: String,
    /// Provider
    pub provider: String,
    /// Base URL
    pub base_url: String,
    /// API Key（None 或空串表示不修改）
    pub api_key: Option<String>,
    /// 上下文窗口
    pub context_window: i64,
    /// 是否启用
    pub enabled: bool,
}

/// 删除模型命令
#[derive(Debug, Clone)]
pub struct DeleteModelCmd {
    /// 模型 id
    pub id: i64,
}

/// 设为默认模型命令
#[derive(Debug, Clone)]
pub struct SetDefaultModelCmd {
    /// 模型 id
    pub id: i64,
}

/// 保存 MCP 服务器命令（按 name 幂等 upsert）
#[derive(Debug, Clone)]
pub struct SaveMcpCmd {
    /// 服务器 id（编辑时携带）
    pub id: Option<i64>,
    /// 服务器名（唯一）
    pub name: String,
    /// 启动命令
    pub command: String,
}

/// 启停 MCP 服务器命令
#[derive(Debug, Clone)]
pub struct ToggleMcpCmd {
    /// 服务器 id
    pub id: i64,
    /// true 启用 / false 禁用
    pub enable: bool,
}

/// 删除 MCP 服务器命令
#[derive(Debug, Clone)]
pub struct DeleteMcpCmd {
    /// 服务器 id
    pub id: i64,
}

/// MCP 市场搜索查询
#[derive(Debug, Clone)]
pub struct SearchMcpMarketQuery {
    /// 关键字（空串 = 只返回精选）
    pub keyword: String,
}

/// 保存权限规则命令（新建按 scope+tool+pattern 幂等 upsert；编辑按 id，沿用原范围）
#[derive(Debug, Clone)]
pub struct SavePermRuleCmd {
    /// 规则 id（编辑时携带）
    pub id: Option<i64>,
    /// 作用域（global/project/session，新建时必填）
    pub scope: String,
    /// 项目 id（scope 为 project/session 时必填）
    pub project_id: Option<i64>,
    /// 会话 id（scope 为 session 时必填）
    pub session_id: Option<i64>,
    /// 工具名（`*` 表示全部）
    pub tool: String,
    /// glob 匹配模式
    pub pattern: String,
    /// 动作（allow/ask/deny）
    pub action: String,
    /// 匹配顺序
    pub sort: i64,
}

/// 删除权限规则命令
#[derive(Debug, Clone)]
pub struct DeletePermRuleCmd {
    /// 规则 id
    pub id: i64,
}
