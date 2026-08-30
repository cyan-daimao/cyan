//! MCP stdio 传输：tokio::process spawn 子进程，newline-delimited JSON-RPC 2.0。
//! 后台任务读 stdout 按 id 分发响应；进程退出 → 全部挂起请求报错收尾。

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use super::jsonrpc::{
    decode_response, encode_notification, encode_request, initialize_params, parse_call_result,
    parse_tools, shell_split, Dispatcher,
};
use super::{McpClient, McpError, McpTool};
use crate::infra::process::{extended_path, resolve_program};

/// stdio MCP 客户端（initialize 握手完成 + tools/list 已缓存）
pub(crate) struct StdioClient {
    stdin: Mutex<ChildStdin>,
    dispatcher: Arc<Dispatcher>,
    tools: Vec<McpTool>,
    /// kill_on_drop：进程句柄仅用于保有所有权，drop 时自动杀进程
    _child: Child,
    reader_task: Mutex<Option<JoinHandle<()>>>,
}

impl StdioClient {
    /// spawn 子进程并完成 initialize → notifications/initialized → tools/list 握手
    pub async fn connect(command: &str) -> Result<Self, McpError> {
        let parts = shell_split(command);
        let (raw_prog, args) = parts
            .split_first()
            .ok_or_else(|| McpError::Spawn("启动命令为空".into()))?;
        // GUI（Dock/Finder）启动时进程 PATH 只含系统目录：先解析程序绝对路径，
        // 再给子进程补全 PATH（npx/uvx 等脚本的 shebang `env node` 依赖它）
        let prog = resolve_program(raw_prog);
        let mut child = Command::new(&prog)
            .args(args)
            .env("PATH", extended_path())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| McpError::Spawn(format!("`{prog}` 启动失败：{e}")))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| McpError::Spawn("无法获取子进程 stdin".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| McpError::Spawn("无法获取子进程 stdout".into()))?;

        let dispatcher = Arc::new(Dispatcher::new());
        let d2 = dispatcher.clone();
        let reader_task = tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        if line.trim().is_empty() {
                            continue;
                        }
                        match decode_response(&line) {
                            Ok(Some(resp)) => d2.resolve(resp),
                            // 服务端通知忽略；非 JSON 行（如服务端打印的日志）仅告警
                            Ok(None) => {}
                            Err(e) => tracing::warn!(error = %e, line = %line, "MCP stdio 帧解析失败"),
                        }
                    }
                    // EOF / 读错误：进程已退出，全部挂起请求报错收尾
                    _ => {
                        d2.fail_all(&McpError::Protocol("MCP 进程已退出".into()));
                        break;
                    }
                }
            }
        });

        let mut client = Self {
            stdin: Mutex::new(stdin),
            dispatcher,
            tools: Vec::new(),
            _child: child,
            reader_task: Mutex::new(Some(reader_task)),
        };
        client.handshake().await?;
        Ok(client)
    }

    /// initialize → notifications/initialized → tools/list
    async fn handshake(&mut self) -> Result<(), McpError> {
        self.request("initialize", initialize_params())
            .await
            .map_err(|e| wrap_handshake_err("initialize", e))?;
        self.notify("notifications/initialized", json!({})).await?;
        let result = self
            .request("tools/list", json!({}))
            .await
            .map_err(|e| wrap_handshake_err("tools/list", e))?;
        self.tools = parse_tools(&result)?;
        Ok(())
    }

    /// 写一帧（请求）：失败时撤销注册
    async fn write_frame(&self, frame: &str) -> Result<(), McpError> {
        let mut stdin = self.stdin.lock().await;
        let r = async {
            stdin.write_all(frame.as_bytes()).await?;
            stdin.write_all(b"\n").await?;
            stdin.flush().await
        }
        .await;
        r.map_err(|e| McpError::Protocol(format!("写入 MCP 进程失败：{e}")))
    }

    /// 发送请求并等待响应（15s 超时由 Dispatcher 兜底）
    async fn request(&self, method: &str, params: Value) -> Result<Value, McpError> {
        let (id, rx) = self.dispatcher.register();
        if let Err(e) = self.write_frame(&encode_request(id, method, params)).await {
            self.dispatcher.cancel(id);
            return Err(e);
        }
        self.dispatcher.wait(id, rx).await
    }

    /// 发送通知（无响应）
    async fn notify(&self, method: &str, params: Value) -> Result<(), McpError> {
        self.write_frame(&encode_notification(method, params)).await
    }
}

/// 握手阶段错误补充上下文
fn wrap_handshake_err(step: &str, e: McpError) -> McpError {
    match e {
        McpError::Timeout(m) => McpError::Timeout(format!("握手 {step} 超时：{m}")),
        other => other,
    }
}

#[async_trait]
impl McpClient for StdioClient {
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

impl Drop for StdioClient {
    fn drop(&mut self) {
        if let Some(task) = self.reader_task.get_mut().take() {
            task.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 写一个 bash 假 MCP server：逐行读 JSON-RPC，按 method 回固定响应
    fn fake_server_script(dir: &std::path::Path) -> std::path::PathBuf {
        let script = dir.join("fake_mcp.sh");
        std::fs::write(
            &script,
            r#"#!/bin/bash
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9][0-9]*\).*/\1/p')
  case "$line" in
    *notifications*) ;;
    *initialize*) printf '{"jsonrpc":"2.0","id":%s,"result":{"protocolVersion":"2024-11-05","capabilities":{"tools":{}},"serverInfo":{"name":"fake","version":"0.1"}}}\n' "$id" ;;
    *tools/list*) printf '{"jsonrpc":"2.0","id":%s,"result":{"tools":[{"name":"echo","description":"回显输入","inputSchema":{"type":"object","properties":{"text":{"type":"string"}},"required":["text"]}},{"name":"ping"}]}}\n' "$id" ;;
    *tools/call*echo*) printf '{"jsonrpc":"2.0","id":%s,"result":{"content":[{"type":"text","text":"pong"}]}}\n' "$id" ;;
    *tools/call*) printf '{"jsonrpc":"2.0","id":%s,"result":{"isError":true,"content":[{"type":"text","text":"unknown tool"}]}}\n' "$id" ;;
  esac
done
"#,
        )
        .unwrap();
        script
    }

    #[tokio::test]
    async fn stdio_initialize_list_call_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let script = fake_server_script(dir.path());
        let client = StdioClient::connect(&format!("bash {}", script.display()))
            .await
            .unwrap();
        // initialize 握手完成 + tools/list 已缓存
        assert_eq!(client.tools().len(), 2);
        assert_eq!(client.tools()[0].name, "echo");
        assert_eq!(client.tools()[0].description, "回显输入");
        // tools/call 成功路径
        let out = client
            .call_tool("echo", json!({"text": "hi"}))
            .await
            .unwrap();
        assert_eq!(out, "pong");
        // tools/call isError 路径 → 友好错误
        let err = client.call_tool("nope", json!({})).await.unwrap_err();
        assert!(matches!(err, McpError::Tool(_)));
        assert!(err.to_string().contains("unknown tool"));
    }

    #[tokio::test]
    async fn stdio_spawn_failure_friendly_error() {
        let err = StdioClient::connect("definitely-not-a-real-binary-xyz123")
            .await
            .err()
            .expect("未知二进制应启动失败");
        assert!(matches!(err, McpError::Spawn(_)));
        assert!(err.to_string().contains("启动失败"));
        // 空命令
        let err = StdioClient::connect("   ").await.err().expect("空命令应失败");
        assert!(matches!(err, McpError::Spawn(_)));
    }

    #[tokio::test]
    async fn stdio_process_exit_surfaces_error() {
        // 启动即退出的假 server：initialize 应报错而非挂起
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("exit.sh");
        std::fs::write(&script, "#!/bin/bash\nexit 1\n").unwrap();
        let err = StdioClient::connect(&format!("bash {}", script.display()))
            .await
            .err()
            .expect("进程退出应握手失败");
        assert!(err.to_string().contains("进程已退出"));
    }
}
