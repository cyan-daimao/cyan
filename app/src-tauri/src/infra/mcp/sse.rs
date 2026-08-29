//! MCP SSE 传输（旧版，mcp-go SSEServer 形态）：
//! GET <url> 长连接收事件流；`endpoint` 事件给出 POST 地址；
//! JSON-RPC 经 POST 发出，响应经 SSE `message` 事件按 id 返回。

use std::sync::Arc;

use async_trait::async_trait;
use futures::StreamExt;
use serde_json::{json, Value};
use tokio::sync::{oneshot, Mutex};
use tokio::task::JoinHandle;

use super::jsonrpc::{
    decode_response, initialize_params, parse_call_result, parse_tools, Dispatcher,
    REQUEST_TIMEOUT,
};
use super::{McpClient, McpError, McpTool};

/// SSE 增量解析器：喂入任意分片文本，产出完整事件 (event, data)
#[derive(Default)]
pub(crate) struct SseParser {
    buf: String,
    event: String,
    data: String,
}

impl SseParser {
    /// 喂入一段文本，返回本次解析出的完整事件列表
    pub fn feed(&mut self, chunk: &str) -> Vec<(String, String)> {
        self.buf.push_str(chunk);
        let mut out = Vec::new();
        while let Some(pos) = self.buf.find('\n') {
            let line: String = self.buf.drain(..=pos).collect();
            let line = line.trim_end_matches(['\n', '\r']);
            if line.is_empty() {
                // 空行 = 事件结束
                if !self.data.is_empty() {
                    let event = if self.event.is_empty() {
                        "message".to_string()
                    } else {
                        std::mem::take(&mut self.event)
                    };
                    out.push((event, std::mem::take(&mut self.data)));
                }
                self.event.clear();
                continue;
            }
            // 注释/心跳行忽略
            if line.starts_with(':') {
                continue;
            }
            if let Some(d) = line.strip_prefix("data:") {
                if !self.data.is_empty() {
                    self.data.push('\n');
                }
                self.data.push_str(d.strip_prefix(' ').unwrap_or(d));
            } else if let Some(e) = line.strip_prefix("event:") {
                self.event = e.strip_prefix(' ').unwrap_or(e).to_string();
            }
        }
        out
    }
}

/// SSE MCP 客户端（initialize 握手完成 + tools/list 已缓存）
pub(crate) struct SseClient {
    http: reqwest::Client,
    post_url: String,
    dispatcher: Arc<Dispatcher>,
    tools: Vec<McpTool>,
    reader_task: Mutex<Option<JoinHandle<()>>>,
}

impl SseClient {
    /// 建立 SSE 长连接，等 endpoint 事件，完成 initialize → notifications/initialized → tools/list
    pub async fn connect(url: &str) -> Result<Self, McpError> {
        let http = reqwest::Client::builder()
            .user_agent("cyan-app")
            .build()
            .map_err(|e| McpError::Http(format!("HTTP client 构建失败：{e}")))?;
        let resp = http
            .get(url)
            .header(reqwest::header::ACCEPT, "text/event-stream")
            .send()
            .await
            .map_err(|e| McpError::Http(format!("SSE 连接失败：{e}")))?;
        if !resp.status().is_success() {
            return Err(McpError::Http(format!(
                "SSE 连接失败：HTTP {}",
                resp.status()
            )));
        }

        let dispatcher = Arc::new(Dispatcher::new());
        let d2 = dispatcher.clone();
        let (endpoint_tx, endpoint_rx) = oneshot::channel::<String>();
        let reader_task = tokio::spawn(async move {
            let mut parser = SseParser::default();
            let mut endpoint_tx = Some(endpoint_tx);
            let mut stream = resp.bytes_stream();
            while let Some(chunk) = stream.next().await {
                let Ok(bytes) = chunk else { break };
                let text = String::from_utf8_lossy(&bytes);
                for (event, data) in parser.feed(&text) {
                    match event.as_str() {
                        "endpoint" => {
                            if let Some(tx) = endpoint_tx.take() {
                                let _ = tx.send(data);
                            }
                        }
                        "message" => match decode_response(&data) {
                            Ok(Some(resp)) => d2.resolve(resp),
                            Ok(None) => {}
                            Err(e) => tracing::warn!(error = %e, "MCP SSE 消息解析失败"),
                        },
                        _ => {}
                    }
                }
            }
            d2.fail_all(&McpError::Http("SSE 流已断开".into()));
        });

        // 等 endpoint 事件（15s 超时）
        let endpoint = match tokio::time::timeout(REQUEST_TIMEOUT, endpoint_rx).await {
            Ok(Ok(ep)) => ep,
            Ok(Err(_)) => return Err(McpError::Http("SSE 流已断开".into())),
            Err(_) => return Err(McpError::Timeout("等待 endpoint 事件超时".into())),
        };
        let post_url = reqwest::Url::parse(url)
            .and_then(|base| base.join(&endpoint))
            .map(|u| u.to_string())
            .map_err(|e| McpError::Protocol(format!("endpoint 地址非法（{endpoint}）：{e}")))?;

        let mut client = Self {
            http,
            post_url,
            dispatcher,
            tools: Vec::new(),
            reader_task: Mutex::new(Some(reader_task)),
        };
        client.handshake().await?;
        Ok(client)
    }

    /// initialize → notifications/initialized → tools/list
    async fn handshake(&mut self) -> Result<(), McpError> {
        self.request("initialize", initialize_params())
            .await
            .map_err(|e| match e {
                McpError::Timeout(m) => McpError::Timeout(format!("握手 initialize 超时：{m}")),
                other => other,
            })?;
        self.notify("notifications/initialized", json!({})).await?;
        let result = self.request("tools/list", json!({})).await?;
        self.tools = parse_tools(&result)?;
        Ok(())
    }

    /// POST 一帧 JSON-RPC
    async fn post(&self, body: Value) -> Result<(), McpError> {
        let resp = self
            .http
            .post(&self.post_url)
            .header(
                reqwest::header::ACCEPT,
                "application/json, text/event-stream",
            )
            .json(&body)
            .send()
            .await
            .map_err(|e| McpError::Http(format!("POST 失败：{e}")))?;
        if !resp.status().is_success() {
            return Err(McpError::Http(format!(
                "MCP POST 返回 HTTP {}",
                resp.status()
            )));
        }
        Ok(())
    }

    /// 发送请求并等待 SSE message 响应（15s 超时由 Dispatcher 兜底）
    async fn request(&self, method: &str, params: Value) -> Result<Value, McpError> {
        let (id, rx) = self.dispatcher.register();
        let body = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        if let Err(e) = self.post(body).await {
            self.dispatcher.cancel(id);
            return Err(e);
        }
        self.dispatcher.wait(id, rx).await
    }

    /// 发送通知（无响应）
    async fn notify(&self, method: &str, params: Value) -> Result<(), McpError> {
        self.post(json!({"jsonrpc": "2.0", "method": method, "params": params}))
            .await
    }
}

#[async_trait]
impl McpClient for SseClient {
    fn tools(&self) -> &[McpTool] {
        &self.tools
    }

    async fn call_tool(&self, tool: &str, args: Value) -> Result<String, McpError> {
        let result = self
            .request("tools/call", json!({"name": tool, "arguments": args}))
            .await?;
        parse_call_result(&result)
    }
}

impl Drop for SseClient {
    fn drop(&mut self) {
        if let Some(task) = self.reader_task.get_mut().take() {
            task.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_parser_endpoint_event() {
        let mut p = SseParser::default();
        // 跨分片喂入
        assert!(p.feed("event: endpo").is_empty());
        let events = p.feed("int\ndata: /messages?session_id=abc\n\n");
        assert_eq!(
            events,
            vec![("endpoint".to_string(), "/messages?session_id=abc".to_string())]
        );
    }

    #[test]
    fn sse_parser_message_event_and_comments() {
        let mut p = SseParser::default();
        let events = p.feed(": keep-alive\n\nevent: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1}\n\n");
        assert_eq!(
            events,
            vec![(
                "message".to_string(),
                "{\"jsonrpc\":\"2.0\",\"id\":1}".to_string()
            )]
        );
    }

    #[test]
    fn sse_parser_default_event_and_multiline_data() {
        let mut p = SseParser::default();
        // 无 event 字段 → 默认 message；多行 data 以 \n 拼接
        let events = p.feed("data: line1\ndata: line2\n\n");
        assert_eq!(events, vec![("message".to_string(), "line1\nline2".to_string())]);
        // CRLF 行尾兼容
        let events = p.feed("event: endpoint\r\ndata: /x\r\n\r\n");
        assert_eq!(events, vec![("endpoint".to_string(), "/x".to_string())]);
    }
}
