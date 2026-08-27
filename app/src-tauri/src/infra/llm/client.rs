//! OpenAI 兼容 SSE 流式客户端（实现 domain LlmGateway 端口）。

use std::time::Duration;

use async_trait::async_trait;
use futures::StreamExt;

use crate::domain::agent::{
    AssistantTurn, CancellationToken, ChatMessage, ChatRequest, ChatRole, ChatToolCall, LlmError,
    LlmGateway, TokenUsage,
};

use super::protocol::{
    ChatChunk, ChatCompletionsReq, ErrorBody, ReqFunction, ReqMessage, ReqTool, ReqToolCall,
    ReqToolFunction, StreamOptions,
};

/// 整体请求超时（含流式读取）
const REQUEST_TIMEOUT: Duration = Duration::from_secs(300);

/// OpenAI 兼容客户端
pub struct OpenAiClient {
    http: reqwest::Client,
}

impl OpenAiClient {
    /// 构造
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .connect_timeout(Duration::from_secs(15))
            .build()
            .expect("reqwest client 构建失败");
        Self { http }
    }
}

impl Default for OpenAiClient {
    fn default() -> Self {
        Self::new()
    }
}

/// domain ChatMessage → 协议消息
fn to_req_message(m: &ChatMessage) -> ReqMessage {
    let role = match m.role {
        ChatRole::User => "user",
        ChatRole::Assistant => "assistant",
        ChatRole::Tool => "tool",
        ChatRole::System => "system",
    };
    ReqMessage {
        role: role.to_string(),
        content: if m.content.is_empty() && !m.tool_calls.is_empty() {
            None
        } else {
            Some(m.content.clone())
        },
        tool_calls: if m.tool_calls.is_empty() {
            None
        } else {
            Some(
                m.tool_calls
                    .iter()
                    .map(|tc| ReqToolCall {
                        id: tc.id.clone(),
                        kind: "function".to_string(),
                        function: ReqFunction {
                            name: tc.name.clone(),
                            arguments: tc.arguments.clone(),
                        },
                    })
                    .collect(),
            )
        },
        tool_call_id: m.tool_call_id.clone(),
    }
}

/// 聚合中的工具调用（SSE 分片拼接）
#[derive(Default)]
struct ToolCallAcc {
    id: String,
    name: String,
    arguments: String,
}

#[async_trait]
impl LlmGateway for OpenAiClient {
    async fn stream_chat(
        &self,
        req: &ChatRequest,
        on_text: &mut (dyn FnMut(String) + Send + '_),
        on_thinking: &mut (dyn FnMut(String) + Send + '_),
        cancel: CancellationToken,
    ) -> Result<AssistantTurn, LlmError> {
        let body = ChatCompletionsReq {
            model: req.model.clone(),
            messages: req.messages.iter().map(to_req_message).collect(),
            tools: req
                .tools
                .iter()
                .map(|t| ReqTool {
                    kind: "function".to_string(),
                    function: ReqToolFunction {
                        name: t.name.clone(),
                        description: t.description.clone(),
                        parameters: t.parameters.clone(),
                    },
                })
                .collect(),
            stream: true,
            stream_options: StreamOptions {
                include_usage: true,
            },
        };
        let url = format!("{}/chat/completions", req.base_url);
        let resp = self
            .http
            .post(&url)
            .bearer_auth(&req.api_key)
            .json(&body)
            .send()
            .await
            .map_err(map_reqwest_err)?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            let message = serde_json::from_str::<ErrorBody>(&text)
                .ok()
                .and_then(|b| b.error.and_then(|e| e.message))
                .unwrap_or(text);
            return Err(if status.is_server_error() {
                LlmError::Server(format!("HTTP {status}：{message}"))
            } else {
                LlmError::Client(format!("HTTP {status}：{message}"))
            });
        }

        // SSE 解析：按行缓冲，处理 `data: <json>`，遇 [DONE] 结束
        let mut turn = AssistantTurn::default();
        let mut accs: Vec<ToolCallAcc> = Vec::new();
        let mut stream = resp.bytes_stream();
        let mut buf = String::new();

        loop {
            if cancel.is_cancelled() {
                return Err(LlmError::Aborted);
            }
            let item = tokio::select! {
                _ = cancel.cancelled() => return Err(LlmError::Aborted),
                item = stream.next() => item,
            };
            let Some(chunk) = item else { break };
            let bytes = chunk.map_err(map_reqwest_err)?;
            buf.push_str(&String::from_utf8_lossy(&bytes));
            // 逐行消费，末尾不完整行留在缓冲
            while let Some(pos) = buf.find('\n') {
                let line: String = buf.drain(..=pos).collect();
                let line = line.trim();
                if line.is_empty() || line.starts_with(':') {
                    continue;
                }
                let Some(data) = line.strip_prefix("data:") else {
                    continue;
                };
                let data = data.trim();
                if data == "[DONE]" {
                    break;
                }
                let parsed = match serde_json::from_str::<ChatChunk>(data) {
                    Ok(c) => c,
                    Err(_) => continue, // 容错：跳过坏包
                };
                if let Some(usage) = parsed.usage {
                    turn.usage = Some(TokenUsage {
                        input: usage.prompt_tokens.unwrap_or(0),
                        output: usage.completion_tokens.unwrap_or(0),
                    });
                }
                for choice in parsed.choices {
                    let Some(delta) = choice.delta else { continue };
                    if let Some(content) = delta.content {
                        if !content.is_empty() {
                            turn.text.push_str(&content);
                            on_text(content);
                        }
                    }
                    // 思考增量：reasoning_content 优先，兼容 reasoning 字段
                    let thinking = delta.reasoning_content.or(delta.reasoning);
                    if let Some(thinking) = thinking {
                        if !thinking.is_empty() {
                            turn.thinking.push_str(&thinking);
                            on_thinking(thinking);
                        }
                    }
                    if let Some(tool_calls) = delta.tool_calls {
                        for tc in tool_calls {
                            let idx = tc.index.unwrap_or(0);
                            while accs.len() <= idx {
                                accs.push(ToolCallAcc::default());
                            }
                            let acc = &mut accs[idx];
                            if let Some(id) = tc.id {
                                acc.id = id;
                            }
                            if let Some(f) = tc.function {
                                if let Some(name) = f.name {
                                    acc.name = name;
                                }
                                if let Some(args) = f.arguments {
                                    acc.arguments.push_str(&args);
                                }
                            }
                        }
                    }
                }
            }
        }

        turn.tool_calls = accs
            .into_iter()
            .map(|a| ChatToolCall {
                id: a.id,
                name: a.name,
                arguments: a.arguments,
            })
            .collect();
        tracing::info!(
            model = %req.model,
            input = turn.usage.map(|u| u.input).unwrap_or(0),
            output = turn.usage.map(|u| u.output).unwrap_or(0),
            tool_calls = turn.tool_calls.len(),
            "LLM 调用完成"
        );
        Ok(turn)
    }
}

/// reqwest 错误 → LlmError 分类（超时/网络可重试）
fn map_reqwest_err(e: reqwest::Error) -> LlmError {
    if e.is_timeout() {
        LlmError::Timeout
    } else if e.is_connect() || e.is_request() {
        LlmError::Network(e.to_string())
    } else {
        LlmError::Client(e.to_string())
    }
}
