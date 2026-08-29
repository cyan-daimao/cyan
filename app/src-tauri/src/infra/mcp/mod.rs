//! MCP 客户端：stdio / SSE 双传输、initialize 握手、tools/list 缓存、tools/call 调用。
//! 协议结构不出 infra 层；连接池 McpPool（进程内共享）经 McpGateway 端口供 agent loop
//! 注入工具（`mcp__<server>__<tool>`）并路由调用。

mod jsonrpc;
mod sse;
mod stdio;

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use serde_json::Value;

use crate::domain::config::McpServer;

/// MCP 工具名前缀：`mcp__<server>__<tool>`
pub const MCP_TOOL_PREFIX: &str = "mcp__";

/// MCP 工具输出截断上限（50KB，防上下文爆炸）
pub const MAX_TOOL_OUTPUT: usize = 50 * 1024;

/// MCP 工具名拼装：`mcp__<server>__<tool>`
pub fn tool_name(server: &str, tool: &str) -> String {
    format!("{MCP_TOOL_PREFIX}{server}__{tool}")
}

/// 解析 `mcp__<server>__<tool>` → (server, tool)；非 MCP 工具名或非法形态返回 None
pub fn parse_tool_name(name: &str) -> Option<(String, String)> {
    let rest = name.strip_prefix(MCP_TOOL_PREFIX)?;
    let (server, tool) = rest.split_once("__")?;
    if server.is_empty() || tool.is_empty() {
        return None;
    }
    Some((server.to_string(), tool.to_string()))
}

/// 握手发现的 MCP 工具（inputSchema 原样透传为 OpenAI function parameters）
#[derive(Debug, Clone, PartialEq)]
pub struct McpTool {
    /// 服务端原始工具名
    pub name: String,
    /// 工具描述
    pub description: String,
    /// 参数 JSON Schema
    pub input_schema: Value,
}

/// MCP 错误（友好文案：启动失败/超时/协议错/网络错/断连/工具执行失败）
#[derive(Debug, Clone, thiserror::Error)]
pub enum McpError {
    /// 进程启动失败
    #[error("启动 MCP 进程失败：{0}")]
    Spawn(String),
    /// 请求/握手超时
    #[error("MCP 请求超时（15s）：{0}")]
    Timeout(String),
    /// 协议错误
    #[error("MCP 协议错误：{0}")]
    Protocol(String),
    /// 网络/HTTP 错误
    #[error("MCP 网络错误：{0}")]
    Http(String),
    /// 服务器未连接或运行中断开
    #[error("MCP 服务器未连接或已断开：{0}")]
    NotConnected(String),
    /// 服务端工具执行返回错误
    #[error("MCP 工具执行失败：{0}")]
    Tool(String),
}

/// 单个 MCP 服务器连接（initialize 握手完成 + tools/list 已缓存）
#[async_trait]
pub trait McpClient: Send + Sync {
    /// 握手时缓存的工具列表
    fn tools(&self) -> &[McpTool];
    /// tools/call 调用
    async fn call_tool(&self, tool: &str, args: Value) -> Result<String, McpError>;
}

/// 按 command 前缀选择传输并完成握手：`http(s)://` → SSE，否则 → stdio 子进程
pub async fn connect(command: &str) -> Result<Box<dyn McpClient>, McpError> {
    if command.starts_with("http://") || command.starts_with("https://") {
        Ok(Box::new(sse::SseClient::connect(command).await?))
    } else {
        Ok(Box::new(stdio::StdioClient::connect(command).await?))
    }
}

/// agent loop 依赖的 MCP 端口（McpPool 实现；测试可注入 mock）
#[async_trait]
pub trait McpGateway: Send + Sync {
    /// 所有已连接 server 的工具（server 名, 工具定义）
    fn connected_tools(&self) -> Vec<(String, McpTool)>;
    /// 路由 tools/call 到对应连接（输出截断 50KB）
    async fn call_tool(&self, server: &str, tool: &str, args: Value) -> Result<String, McpError>;
    /// 握手并注册连接，返回发现的工具数
    async fn connect(&self, server_name: &str, command: &str) -> Result<usize, McpError>;
    /// 断开并清理连接（幂等）
    async fn disconnect(&self, server_name: &str);
}

/// 进程内 MCP 连接池（server name → 活跃 client），App 内 Arc 共享
#[derive(Default)]
pub struct McpPool {
    clients: RwLock<HashMap<String, Arc<dyn McpClient>>>,
}

impl McpPool {
    /// 构造空连接池
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl McpGateway for McpPool {
    fn connected_tools(&self) -> Vec<(String, McpTool)> {
        self.clients
            .read()
            .expect("mcp 连接池锁中毒")
            .iter()
            .flat_map(|(name, c)| {
                c.tools()
                    .iter()
                    .cloned()
                    .map(|t| (name.clone(), t))
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    async fn call_tool(&self, server: &str, tool: &str, args: Value) -> Result<String, McpError> {
        let client = self
            .clients
            .read()
            .expect("mcp 连接池锁中毒")
            .get(server)
            .cloned()
            .ok_or_else(|| McpError::NotConnected(server.to_string()))?;
        // 锁不跨 await：先取出 Arc 再调用
        let out = client.call_tool(tool, args).await?;
        Ok(truncate_output(&out, MAX_TOOL_OUTPUT))
    }

    async fn connect(&self, server_name: &str, command: &str) -> Result<usize, McpError> {
        // 同名已有连接先清理（重连刷新工具列表）
        self.disconnect(server_name).await;
        let client = connect(command).await?;
        let n = client.tools().len();
        self.clients
            .write()
            .expect("mcp 连接池锁中毒")
            .insert(server_name.to_string(), Arc::from(client));
        Ok(n)
    }

    async fn disconnect(&self, server_name: &str) {
        // drop client 即断开：stdio 杀子进程 / SSE 中止事件流
        self.clients
            .write()
            .expect("mcp 连接池锁中毒")
            .remove(server_name);
    }
}

/// 截断输出到上限（字符边界安全）
fn truncate_output(s: &str, limit: usize) -> String {
    if s.len() <= limit {
        return s.to_string();
    }
    let mut end = limit;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…\n[输出已截断]", &s[..end])
}

/// toggle enable 的真实握手入口：状态机流转 + 建立连接 + 工具数写回。
/// 成功 → connected（tools=发现数）；失败 → error（原因落 last_error）。
/// 允许 connected 状态重复 enable（先断旧连接触发重连，刷新工具列表）。
pub async fn handshake(
    server: &mut McpServer,
    gateway: &Arc<dyn McpGateway>,
) -> Result<(), McpError> {
    gateway.disconnect(&server.name).await;
    server.disable();
    server
        .connect()
        .map_err(|e| McpError::Protocol(e.to_string()))?;
    if server.command.trim().is_empty() {
        server.mark_error("启动命令为空".into());
        return Err(McpError::Spawn("启动命令为空".into()));
    }
    match gateway.connect(&server.name, &server.command).await {
        Ok(n) => {
            server
                .mark_connected(n as i64)
                .map_err(|e| McpError::Protocol(e.to_string()))?;
            Ok(())
        }
        Err(e) => {
            server.mark_error(e.to_string());
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Mutex;

    #[test]
    fn tool_name_roundtrip() {
        assert_eq!(tool_name("fs", "read"), "mcp__fs__read");
        assert_eq!(
            parse_tool_name("mcp__fs__read"),
            Some(("fs".to_string(), "read".to_string()))
        );
    }

    #[test]
    fn parse_tool_name_rejects_illegal() {
        assert_eq!(parse_tool_name("Read"), None);
        assert_eq!(parse_tool_name("mcp__"), None);
        assert_eq!(parse_tool_name("mcp__fs__"), None);
        assert_eq!(parse_tool_name("mcp____read"), None);
        assert_eq!(parse_tool_name("mcpfsread"), None);
    }

    #[test]
    fn truncate_output_at_50kb() {
        let s = "汉".repeat(MAX_TOOL_OUTPUT);
        let out = truncate_output(&s, MAX_TOOL_OUTPUT);
        assert!(out.contains("[输出已截断]"));
        assert!(out.len() <= MAX_TOOL_OUTPUT + 32);
        let short = "abc";
        assert_eq!(truncate_output(short, MAX_TOOL_OUTPUT), "abc");
    }

    /// mock client：固定工具列表 + 记录调用
    struct MockClient {
        tools: Vec<McpTool>,
        output: String,
        calls: Mutex<Vec<(String, Value)>>,
    }

    #[async_trait]
    impl McpClient for MockClient {
        fn tools(&self) -> &[McpTool] {
            &self.tools
        }
        async fn call_tool(&self, tool: &str, args: Value) -> Result<String, McpError> {
            self.calls
                .lock()
                .unwrap()
                .push((tool.to_string(), args));
            Ok(self.output.clone())
        }
    }

    fn mock_client(output: &str) -> Arc<dyn McpClient> {
        Arc::new(MockClient {
            tools: vec![McpTool {
                name: "echo".into(),
                description: "回显".into(),
                input_schema: json!({"type": "object"}),
            }],
            output: output.into(),
            calls: Mutex::new(Vec::new()),
        })
    }

    #[tokio::test]
    async fn pool_lists_and_routes_calls() {
        let pool = McpPool::new();
        pool.clients
            .write()
            .unwrap()
            .insert("fs".into(), mock_client("pong"));
        let tools = pool.connected_tools();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].0, "fs");
        assert_eq!(tools[0].1.name, "echo");
        let out = pool
            .call_tool("fs", "echo", json!({"text": "hi"}))
            .await
            .unwrap();
        assert_eq!(out, "pong");
        // 断开后调用 → NotConnected（不 panic）
        pool.disconnect("fs").await;
        assert!(pool.connected_tools().is_empty());
        let err = pool.call_tool("fs", "echo", json!({})).await.unwrap_err();
        assert!(matches!(err, McpError::NotConnected(_)));
    }

    #[tokio::test]
    async fn pool_truncates_large_output() {
        let pool = McpPool::new();
        pool.clients
            .write()
            .unwrap()
            .insert("big".into(), mock_client(&"x".repeat(MAX_TOOL_OUTPUT * 2)));
        let out = pool.call_tool("big", "echo", json!({})).await.unwrap();
        assert!(out.contains("[输出已截断]"));
    }

    #[tokio::test]
    async fn handshake_marks_connected_or_error() {
        // 空命令 → error
        let gateway: Arc<dyn McpGateway> = Arc::new(McpPool::new());
        let mut s = McpServer::new("bad".into(), "  ".into(), chrono::NaiveDateTime::default());
        assert!(handshake(&mut s, &gateway).await.is_err());
        assert_eq!(s.status, crate::domain::config::McpStatus::Error);
        assert_eq!(s.last_error.as_deref(), Some("启动命令为空"));
    }
}
