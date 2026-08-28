//! McpServer：MCP 服务器状态机（disabled → connecting → connected / error）。

use chrono::NaiveDateTime;

use crate::domain::DomainError;

/// MCP 服务器状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpStatus {
    /// 禁用
    Disabled,
    /// 连接中
    Connecting,
    /// 已连接
    Connected,
    /// 连接失败
    Error,
}

impl McpStatus {
    /// 存储字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Connecting => "connecting",
            Self::Connected => "connected",
            Self::Error => "error",
        }
    }

    /// 从存储字符串解析
    pub fn parse(s: &str) -> Self {
        match s {
            "connecting" => Self::Connecting,
            "connected" => Self::Connected,
            "error" => Self::Error,
            _ => Self::Disabled,
        }
    }
}

/// MCP 服务器配置
#[derive(Debug, Clone)]
pub struct McpServer {
    /// 主键 id（插入后回填）
    pub id: i64,
    /// 服务器名（唯一）
    pub name: String,
    /// 启动命令（stdio）
    pub command: String,
    /// 连接状态
    pub status: McpStatus,
    /// 握手发现的工具数
    pub tools: i64,
    /// 最近失败原因
    pub last_error: Option<String>,
    /// 来源插件名（None = 用户自建）
    pub plugin_origin: Option<String>,
    /// 创建时间
    pub created_at: NaiveDateTime,
    /// 更新时间
    pub updated_at: NaiveDateTime,
}

impl McpServer {
    /// 新建（未持久化，id 待回填）
    pub fn new(name: String, command: String, now: NaiveDateTime) -> Self {
        Self {
            id: 0,
            name,
            command,
            status: McpStatus::Disabled,
            tools: 0,
            last_error: None,
            plugin_origin: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// 校验：名称/命令非空
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.name.trim().is_empty() {
            return Err(DomainError::Validation("MCP 服务器名不能为空".into()));
        }
        if self.command.trim().is_empty() {
            return Err(DomainError::Validation("MCP 启动命令不能为空".into()));
        }
        Ok(())
    }

    /// 发起连接：disabled/error → connecting
    pub fn connect(&mut self) -> Result<(), DomainError> {
        match self.status {
            McpStatus::Disabled | McpStatus::Error => {
                self.status = McpStatus::Connecting;
                Ok(())
            }
            McpStatus::Connecting => Err(DomainError::State("正在连接中".into())),
            McpStatus::Connected => Ok(()),
        }
    }

    /// 连接成功：connecting → connected，记录发现的工具数
    pub fn mark_connected(&mut self, tools: i64) -> Result<(), DomainError> {
        if self.status != McpStatus::Connecting {
            return Err(DomainError::State("当前状态不能标记为已连接".into()));
        }
        self.status = McpStatus::Connected;
        self.tools = tools;
        self.last_error = None;
        Ok(())
    }

    /// 连接失败：→ error，记录原因
    pub fn mark_error(&mut self, reason: String) {
        self.status = McpStatus::Error;
        self.last_error = Some(reason);
    }

    /// 禁用：任意状态 → disabled
    pub fn disable(&mut self) {
        self.status = McpStatus::Disabled;
        self.tools = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_flow() {
        let mut s = McpServer::new("fs".into(), "npx mcp-fs".into(), NaiveDateTime::default());
        assert_eq!(s.status, McpStatus::Disabled);
        s.connect().unwrap();
        assert_eq!(s.status, McpStatus::Connecting);
        assert!(s.connect().is_err());
        s.mark_connected(3).unwrap();
        assert_eq!(s.status, McpStatus::Connected);
        assert_eq!(s.tools, 3);
        s.disable();
        assert_eq!(s.status, McpStatus::Disabled);
        assert_eq!(s.tools, 0);
    }

    #[test]
    fn error_flow() {
        let mut s = McpServer::new("fs".into(), "npx mcp-fs".into(), NaiveDateTime::default());
        s.mark_error("握手超时".into());
        assert_eq!(s.status, McpStatus::Error);
        assert_eq!(s.last_error.as_deref(), Some("握手超时"));
        s.connect().unwrap();
        assert_eq!(s.status, McpStatus::Connecting);
        assert!(s.mark_connected(0).is_ok());
    }

    #[test]
    fn validate_empty() {
        let s = McpServer::new(" ".into(), "cmd".into(), NaiveDateTime::default());
        assert!(matches!(s.validate(), Err(DomainError::Validation(_))));
    }
}
