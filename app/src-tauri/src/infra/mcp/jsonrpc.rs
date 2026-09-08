//! JSON-RPC 2.0 帧编解码（纯函数）+ 按 id 分发的请求分发器 + MCP 结果解析。
//! newline-delimited JSON-RPC（stdio）与 SSE message 事件共用同一套帧格式。

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::sync::oneshot;

use super::{McpError, McpTool};

/// 单次请求超时（15s）
pub(crate) const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// 编码 JSON-RPC 请求帧（单行 JSON，无换行）
pub(crate) fn encode_request(id: i64, method: &str, params: Value) -> String {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string()
}

/// 编码 JSON-RPC 通知帧（无 id）
pub(crate) fn encode_notification(method: &str, params: Value) -> String {
    json!({"jsonrpc": "2.0", "method": method, "params": params}).to_string()
}

/// 一条按 id 关联到请求的 JSON-RPC 响应
#[derive(Debug)]
pub(crate) struct RpcResponse {
    /// 请求 id
    pub id: i64,
    /// result 或 RPC 错误
    pub result: Result<Value, McpError>,
}

/// 解码一帧：响应 → Some(RpcResponse)；服务端通知/请求（无配对 id）→ None；非法帧 → Err
pub(crate) fn decode_response(line: &str) -> Result<Option<RpcResponse>, McpError> {
    let v: Value = serde_json::from_str(line)
        .map_err(|e| McpError::Protocol(format!("响应不是合法 JSON：{e}")))?;
    // 服务端主动下发的通知/请求（带 method）：忽略
    if v.get("method").is_some() {
        return Ok(None);
    }
    let id = match v.get("id") {
        None | Some(Value::Null) => return Ok(None),
        Some(i) => i
            .as_i64()
            .or_else(|| i.as_str().and_then(|s| s.parse().ok()))
            .ok_or_else(|| McpError::Protocol("响应 id 非法".into()))?,
    };
    if v.get("jsonrpc").and_then(|j| j.as_str()) != Some("2.0") {
        return Err(McpError::Protocol("响应缺少 jsonrpc=2.0".into()));
    }
    if let Some(err) = v.get("error") {
        let code = err.get("code").and_then(|c| c.as_i64()).unwrap_or(0);
        let msg = err
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("未知错误");
        return Ok(Some(RpcResponse {
            id,
            result: Err(McpError::Protocol(format!("RPC 错误 {code}：{msg}"))),
        }));
    }
    match v.get("result") {
        Some(r) => Ok(Some(RpcResponse {
            id,
            result: Ok(r.clone()),
        })),
        None => Err(McpError::Protocol("响应既无 result 也无 error".into())),
    }
}

/// initialize 请求参数
pub(crate) fn initialize_params() -> Value {
    json!({
        "protocolVersion": "2024-11-05",
        "capabilities": {},
        "clientInfo": {"name": "cyan", "version": env!("CARGO_PKG_VERSION")},
    })
}

/// tools/list result → 工具列表（缺 name 的条目跳过）
pub(crate) fn parse_tools(result: &Value) -> Result<Vec<McpTool>, McpError> {
    let tools = result
        .get("tools")
        .and_then(|t| t.as_array())
        .ok_or_else(|| McpError::Protocol("tools/list 响应缺少 tools 数组".into()))?;
    Ok(tools
        .iter()
        .filter_map(|t| {
            let name = t.get("name")?.as_str()?;
            Some(McpTool {
                name: name.to_string(),
                description: t
                    .get("description")
                    .and_then(|d| d.as_str())
                    .unwrap_or("")
                    .to_string(),
                input_schema: t
                    .get("inputSchema")
                    .cloned()
                    .unwrap_or(json!({"type": "object"})),
            })
        })
        .collect())
}

/// tools/call result → 文本输出（拼接 text 项；image 项落盘为本地文件并输出 markdown 链接；
/// isError=true 转为 McpError::Tool）
///
/// image 落盘：MCP 图片（如 Playwright MCP 的 browser_take_screenshot）体积大且
/// base64 不适合直出 LLM 上下文——存到 `~/.cyan/screenshots/mcp-<seq>.png`，
/// 文本里给 markdown 链接（agent 可引用路径，前端可渲染预览）。
pub(crate) fn parse_call_result(result: &Value) -> Result<String, McpError> {
    let is_error = result
        .get("isError")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let mut parts: Vec<String> = Vec::new();
    if let Some(items) = result.get("content").and_then(|c| c.as_array()) {
        for i in items {
            match i.get("type").and_then(|t| t.as_str()) {
                Some("text") => {
                    if let Some(t) = i.get("text").and_then(|t| t.as_str()) {
                        parts.push(t.to_string());
                    }
                }
                Some("image") => {
                    // image block：{type:"image", data:<base64>, mimeType:"image/png"}
                    let data = i.get("data").and_then(|d| d.as_str()).unwrap_or("");
                    let mime = i
                        .get("mimeType")
                        .and_then(|m| m.as_str())
                        .unwrap_or("image/png");
                    if data.is_empty() {
                        parts.push("[空图片内容]".to_string());
                    } else {
                        match save_mcp_image(data, mime) {
                            Ok(path) => {
                                parts.push(format!("图片已保存：{path}"));
                            }
                            Err(e) => {
                                parts.push(format!("[图片落盘失败：{e}]"));
                            }
                        }
                    }
                }
                Some(other) => parts.push(format!("[不支持的内容类型：{other}]")),
                None => {}
            }
        }
    }
    let text = parts.join("\n");
    if is_error {
        Err(McpError::Tool(if text.is_empty() {
            "工具返回错误（无详情）".into()
        } else {
            text
        }))
    } else {
        Ok(text)
    }
}

/// MCP image block 落盘：`~/.cyan/screenshots/mcp-<时间戳>-<序号>.<ext>`
/// 返回保存后的绝对路径字符串
fn save_mcp_image(data_b64: &str, mime: &str) -> Result<String, String> {
    let ext = match mime {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/webp" => "webp",
        "image/gif" => "gif",
        _ => return Err(format!("未知图片类型：{mime}")),
    };
    let bytes = crate::infra::browser::page::base64_decode_pub(data_b64)
        .map_err(|e| e.to_string())?;
    let dir = crate::infra::browser::page::screenshot_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let name = format!(
        "mcp-{}-{seq}.{ext}",
        chrono::Local::now().format("%Y%m%d-%H%M%S")
    );
    let path = dir.join(name);
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

/// 简单 shell 分词：按空白切分，支持单/双引号包裹含空格的参数
pub(crate) fn shell_split(cmd: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut has_token = false;
    for c in cmd.chars() {
        match quote {
            Some(q) => {
                if c == q {
                    quote = None;
                } else {
                    cur.push(c);
                }
            }
            None => {
                if c == '"' || c == '\'' {
                    quote = Some(c);
                    has_token = true;
                } else if c.is_whitespace() {
                    if has_token || !cur.is_empty() {
                        out.push(std::mem::take(&mut cur));
                        has_token = false;
                    }
                } else {
                    cur.push(c);
                    has_token = true;
                }
            }
        }
    }
    if has_token || !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// 请求分发器：分配自增 id、按 id 投递响应、超时/断连兜底
pub(crate) struct Dispatcher {
    next_id: AtomicI64,
    pending: Mutex<HashMap<i64, oneshot::Sender<Result<Value, McpError>>>>,
}

impl Dispatcher {
    /// 构造
    pub fn new() -> Self {
        Self {
            next_id: AtomicI64::new(1),
            pending: Mutex::new(HashMap::new()),
        }
    }

    /// 注册一个待响应请求，返回 (id, 接收端)
    pub fn register(&self) -> (i64, oneshot::Receiver<Result<Value, McpError>>) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending
            .lock()
            .expect("mcp pending 锁中毒")
            .insert(id, tx);
        (id, rx)
    }

    /// 请求发出失败时撤销注册
    pub fn cancel(&self, id: i64) {
        self.pending.lock().expect("mcp pending 锁中毒").remove(&id);
    }

    /// 投递一条响应（无匹配 id 时丢弃）
    pub fn resolve(&self, resp: RpcResponse) {
        let tx = self
            .pending
            .lock()
            .expect("mcp pending 锁中毒")
            .remove(&resp.id);
        if let Some(tx) = tx {
            let _ = tx.send(resp.result);
        }
    }

    /// 连接断开：让全部挂起请求以同一错误收尾
    pub fn fail_all(&self, err: &McpError) {
        let mut map = self.pending.lock().expect("mcp pending 锁中毒");
        for (_, tx) in map.drain() {
            let _ = tx.send(Err(err.clone()));
        }
    }

    /// 等待响应（15s 超时，超时自动清理注册）
    pub async fn wait(
        &self,
        id: i64,
        rx: oneshot::Receiver<Result<Value, McpError>>,
    ) -> Result<Value, McpError> {
        match tokio::time::timeout(REQUEST_TIMEOUT, rx).await {
            Ok(Ok(r)) => r,
            Ok(Err(_)) => Err(McpError::Protocol("响应通道已关闭".into())),
            Err(_) => {
                self.cancel(id);
                Err(McpError::Timeout("等待服务端响应超时".into()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_request_frame_shape() {
        let frame = encode_request(7, "tools/list", json!({}));
        let v: Value = serde_json::from_str(&frame).unwrap();
        assert_eq!(v["jsonrpc"], "2.0");
        assert_eq!(v["id"], 7);
        assert_eq!(v["method"], "tools/list");
        assert!(v.get("params").is_some());
        assert!(!frame.contains('\n'), "帧必须是单行");
    }

    #[test]
    fn encode_notification_has_no_id() {
        let frame = encode_notification("notifications/initialized", json!({}));
        let v: Value = serde_json::from_str(&frame).unwrap();
        assert_eq!(v["method"], "notifications/initialized");
        assert!(v.get("id").is_none());
    }

    #[test]
    fn decode_result_response() {
        let r = decode_response(r#"{"jsonrpc":"2.0","id":3,"result":{"tools":[]}}"#)
            .unwrap()
            .unwrap();
        assert_eq!(r.id, 3);
        assert_eq!(r.result.unwrap(), json!({"tools": []}));
    }

    #[test]
    fn decode_error_response_maps_rpc_error() {
        let r = decode_response(r#"{"jsonrpc":"2.0","id":4,"error":{"code":-32601,"message":"Method not found"}}"#)
            .unwrap()
            .unwrap();
        assert_eq!(r.id, 4);
        let err = r.result.unwrap_err();
        assert!(err.to_string().contains("-32601"));
        assert!(err.to_string().contains("Method not found"));
    }

    #[test]
    fn decode_ignores_server_notifications_and_requests() {
        assert!(decode_response(r#"{"jsonrpc":"2.0","method":"notifications/progress","params":{}}"#)
            .unwrap()
            .is_none());
        // 无 id 的非 method 帧同样忽略
        assert!(decode_response(r#"{"jsonrpc":"2.0"}"#).unwrap().is_none());
        // 字符串 id 兼容解析
        let r = decode_response(r#"{"jsonrpc":"2.0","id":"9","result":{}}"#)
            .unwrap()
            .unwrap();
        assert_eq!(r.id, 9);
    }

    #[test]
    fn decode_rejects_garbage_and_bad_frames() {
        assert!(decode_response("not json").is_err());
        assert!(decode_response(r#"{"id":1,"result":{}}"#).is_err()); // 缺 jsonrpc=2.0
        assert!(decode_response(r#"{"jsonrpc":"2.0","id":1}"#).is_err()); // 无 result/error
    }

    #[test]
    fn parse_tools_extracts_name_desc_schema() {
        let result = json!({"tools": [
            {"name": "echo", "description": "回显", "inputSchema": {"type": "object", "properties": {"text": {"type": "string"}}}},
            {"name": "ping"},
            {"description": "缺 name 跳过"}
        ]});
        let tools = parse_tools(&result).unwrap();
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].name, "echo");
        assert_eq!(tools[0].description, "回显");
        assert_eq!(tools[0].input_schema["properties"]["text"]["type"], "string");
        assert_eq!(tools[1].name, "ping");
        assert_eq!(tools[1].input_schema, json!({"type": "object"}));
        assert!(parse_tools(&json!({})).is_err());
    }

    #[test]
    fn parse_call_result_joins_text_and_flags_error() {
        let ok = json!({"content": [{"type": "text", "text": "a"}, {"type": "text", "text": "b"}]});
        assert_eq!(parse_call_result(&ok).unwrap(), "a\nb");
        let err = json!({"isError": true, "content": [{"type": "text", "text": "boom"}]});
        let e = parse_call_result(&err).unwrap_err();
        assert!(matches!(e, McpError::Tool(_)));
        assert!(e.to_string().contains("boom"));
        let other = json!({"content": [{"type": "audio", "data": "..."}]});
        assert!(parse_call_result(&other).unwrap().contains("不支持的内容类型"));
    }

    #[test]
    fn parse_call_result_saves_image_to_disk() {
        // 1x1 PNG 的 base64
        let png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";
        let result = json!({"content": [
            {"type": "text", "text": "截图完成"},
            {"type": "image", "data": png, "mimeType": "image/png"}
        ]});
        let out = parse_call_result(&result).unwrap();
        assert!(out.contains("截图完成"));
        assert!(out.contains("图片已保存："), "应输出保存路径：{out}");
        // 提取路径验证文件真实存在且是 PNG
        let path = out
            .split("图片已保存：")
            .nth(1)
            .unwrap()
            .trim();
        let bytes = std::fs::read(path).unwrap();
        assert_eq!(&bytes[..4], b"\x89PNG", "落盘文件应是 PNG");
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn parse_call_result_image_empty_data_is_placeholder() {
        let result = json!({"content": [{"type": "image", "data": "", "mimeType": "image/png"}]});
        let out = parse_call_result(&result).unwrap();
        assert!(out.contains("空图片内容"));
        // 未知 mime → 落盘失败提示
        let result = json!({"content": [{"type": "image", "data": "aGk=", "mimeType": "video/mp4"}]});
        let out = parse_call_result(&result).unwrap();
        assert!(out.contains("图片落盘失败"));
    }

    #[test]
    fn shell_split_handles_quotes() {
        assert_eq!(shell_split("npx -y @scope/pkg"), vec!["npx", "-y", "@scope/pkg"]);
        assert_eq!(
            shell_split("node server.js --name \"my server\""),
            vec!["node", "server.js", "--name", "my server"]
        );
        assert_eq!(shell_split("echo 'a b'"), vec!["echo", "a b"]);
        assert!(shell_split("   ").is_empty());
    }

    #[tokio::test]
    async fn dispatcher_resolves_and_fails_all() {
        let d = Dispatcher::new();
        let (id, rx) = d.register();
        d.resolve(RpcResponse {
            id,
            result: Ok(json!({"ok": true})),
        });
        assert_eq!(d.wait(id, rx).await.unwrap(), json!({"ok": true}));

        // 无匹配 id 的响应被丢弃
        d.resolve(RpcResponse {
            id: 999,
            result: Ok(json!(null)),
        });

        // fail_all 让挂起请求以错误收尾
        let (id2, rx2) = d.register();
        d.fail_all(&McpError::Protocol("连接断开".into()));
        let err = d.wait(id2, rx2).await.unwrap_err();
        assert!(err.to_string().contains("连接断开"));
    }
}
