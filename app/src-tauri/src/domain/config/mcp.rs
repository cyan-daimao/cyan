//! McpServer：MCP 服务器状态机（disabled → connecting → connected / error）。
//! 传输方式：stdio = 本地子进程（command 为启动命令）/ sse = 远程服务（command 为 URL）。

use std::collections::HashMap;

use chrono::NaiveDateTime;

use crate::domain::DomainError;

/// MCP 传输方式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpTransport {
    /// 本地子进程（command 为启动命令）
    Stdio,
    /// 远程 SSE 服务（command 为服务 URL）
    Sse,
}

impl McpTransport {
    /// 存储字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Stdio => "stdio",
            Self::Sse => "sse",
        }
    }

    /// 从存储字符串解析（非法/缺省归 stdio，向后兼容旧数据）
    pub fn parse(s: &str) -> Self {
        match s {
            "sse" => Self::Sse,
            _ => Self::Stdio,
        }
    }

    /// 判断是否为远程服务地址（迁移回填/插件 sidecar 安装判定用）
    pub fn is_remote_url(s: &str) -> bool {
        s.starts_with("http://") || s.starts_with("https://")
    }
}

/// 解析 JSON 对象形式的请求头文本（非法 JSON / 非 JSON 对象返回 None）
fn parse_headers_json(text: &str) -> Option<HashMap<String, String>> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let obj = v.as_object()?;
    Some(
        obj.iter()
            .map(|(k, val)| {
                let s = val
                    .as_str()
                    .map(String::from)
                    .unwrap_or_else(|| val.to_string());
                (k.clone(), s)
            })
            .collect(),
    )
}

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
    /// 传输方式（stdio/sse）
    pub transport: McpTransport,
    /// stdio：启动命令；sse：服务 URL（字段语义随 transport 切换）
    pub command: String,
    /// 远程服务请求头（JSON 对象文本，如 {"Authorization":"Bearer x"}；stdio 忽略）
    pub headers: String,
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
    /// 软删时间（未删除为 None，回收站展示用）
    pub deleted_at: Option<NaiveDateTime>,
}

impl McpServer {
    /// 新建 stdio 服务器（未持久化，id 待回填；sse 用 with_transport）
    pub fn new(name: String, command: String, now: NaiveDateTime) -> Self {
        Self::with_transport(name, McpTransport::Stdio, command, now)
    }

    /// 按传输方式新建（headers 初始为空对象文本）
    pub fn with_transport(
        name: String,
        transport: McpTransport,
        command: String,
        now: NaiveDateTime,
    ) -> Self {
        Self {
            id: 0,
            name,
            transport,
            command,
            headers: "{}".to_string(),
            status: McpStatus::Disabled,
            tools: 0,
            last_error: None,
            plugin_origin: None,
            created_at: now,
            updated_at: now,
            deleted_at: None,
        }
    }

    /// 远程服务请求头（非法 JSON / 非 JSON 对象归空 map，保证下游稳定）
    pub fn headers_map(&self) -> HashMap<String, String> {
        parse_headers_json(&self.headers).unwrap_or_default()
    }

    /// 校验：名称/命令非空；sse 时命令必须是 http(s) URL、headers 必须为 JSON 对象
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.name.trim().is_empty() {
            return Err(DomainError::Validation("MCP 服务器名不能为空".into()));
        }
        if self.command.trim().is_empty() {
            return Err(DomainError::Validation("MCP 启动命令不能为空".into()));
        }
        if self.transport == McpTransport::Sse && !McpTransport::is_remote_url(&self.command) {
            return Err(DomainError::Validation(
                "远程服务地址必须以 http:// 或 https:// 开头".into(),
            ));
        }
        if parse_headers_json(&self.headers).is_none() {
            return Err(DomainError::Validation(
                "请求头必须是合法的 JSON 对象（如 {\"Authorization\":\"Bearer x\"}）".into(),
            ));
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

    #[test]
    fn transport_parse_and_url_check() {
        assert_eq!(McpTransport::parse("sse"), McpTransport::Sse);
        assert_eq!(McpTransport::parse("stdio"), McpTransport::Stdio);
        assert_eq!(McpTransport::parse("junk"), McpTransport::Stdio);
        assert!(McpTransport::is_remote_url("https://x.dev/sse"));
        assert!(McpTransport::is_remote_url("http://127.0.0.1:54554/sse"));
        assert!(!McpTransport::is_remote_url("npx -y foo"));
    }

    #[test]
    fn validate_sse_requires_url_and_object_headers() {
        let mut s = McpServer::with_transport(
            "db".into(),
            McpTransport::Sse,
            "http://127.0.0.1:54554/sse".into(),
            NaiveDateTime::default(),
        );
        assert!(s.validate().is_ok());
        s.headers = r#"{"Authorization":"Bearer t"}"#.into();
        assert!(s.validate().is_ok());
        s.headers = "[]".into();
        assert!(matches!(s.validate(), Err(DomainError::Validation(_))));
        s.headers = "not json".into();
        assert!(matches!(s.validate(), Err(DomainError::Validation(_))));
        s.headers = "{}".into();
        s.command = "npx -y foo".into();
        assert!(matches!(s.validate(), Err(DomainError::Validation(_))));
        // stdio 忽略 headers 内容
        let st = McpServer::new("fs".into(), "npx -y foo".into(), NaiveDateTime::default());
        assert!(st.validate().is_ok());
    }

    #[test]
    fn headers_map_tolerates_bad_json() {
        let mut s = McpServer::new("x".into(), "c".into(), NaiveDateTime::default());
        s.headers = r#"{"A":"1","B":"2"}"#.into();
        assert_eq!(s.headers_map().get("A").map(String::as_str), Some("1"));
        s.headers = "broken".into();
        assert!(s.headers_map().is_empty());
    }
}
