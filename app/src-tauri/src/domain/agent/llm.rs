//! LLM 调用端口（infra/llm 实现 OpenAI 兼容 SSE 客户端，协议对象不出 infra 层）。

use async_trait::async_trait;

use super::{CancellationToken, TokenUsage, ToolSpec};

/// 对话角色
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatRole {
    /// 用户
    User,
    /// 助手
    Assistant,
    /// 工具结果
    Tool,
    /// 系统
    System,
}

/// LLM 请求的工具调用（assistant 消息携带）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatToolCall {
    /// 调用 id
    pub id: String,
    /// 工具名
    pub name: String,
    /// 参数 JSON 字符串
    pub arguments: String,
}

/// 对话消息（跨层传输结构）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
    /// 角色
    pub role: ChatRole,
    /// 文本内容
    pub content: String,
    /// assistant 消息的工具调用
    pub tool_calls: Vec<ChatToolCall>,
    /// tool 消息对应的调用 id
    pub tool_call_id: Option<String>,
}

impl ChatMessage {
    /// 构造纯文本消息
    pub fn text(role: ChatRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }
}

/// LLM 对话请求
#[derive(Debug, Clone)]
pub struct ChatRequest {
    /// Base URL（OpenAI 兼容）
    pub base_url: String,
    /// API Key（真实值，来自 keychain）
    pub api_key: String,
    /// 模型名
    pub model: String,
    /// 消息列表
    pub messages: Vec<ChatMessage>,
    /// 可用工具
    pub tools: Vec<ToolSpec>,
}

/// 一轮助手输出（流式聚合结果）
#[derive(Debug, Clone, Default)]
pub struct AssistantTurn {
    /// 聚合文本
    pub text: String,
    /// 聚合思考过程（推理模型 reasoning_content；非推理模型为空）
    pub thinking: String,
    /// 工具调用列表
    pub tool_calls: Vec<ChatToolCall>,
    /// token 用量（部分提供商末包才返回）
    pub usage: Option<TokenUsage>,
}

/// LLM 错误（分类：超时/5xx 可重试）
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    /// 请求超时（可重试）
    #[error("LLM 请求超时")]
    Timeout,
    /// 服务端 5xx（可重试）
    #[error("LLM 服务端错误：{0}")]
    Server(String),
    /// 客户端 4xx / 协议错误（不可重试）
    #[error("LLM 客户端错误：{0}")]
    Client(String),
    /// 网络错误（可重试）
    #[error("LLM 网络错误：{0}")]
    Network(String),
    /// 已被中断
    #[error("LLM 调用已中断")]
    Aborted,
}

impl LlmError {
    /// 是否可重试（超时/5xx/网络抖动）
    pub fn retryable(&self) -> bool {
        matches!(self, Self::Timeout | Self::Server(_) | Self::Network(_))
    }
}

/// LLM 调用端口
#[async_trait]
pub trait LlmGateway: Send + Sync {
    /// 流式对话：文本增量经 on_text、思考增量经 on_thinking 回调吐出，返回聚合结果；cancel 触发后以 Aborted 收尾
    async fn stream_chat(
        &self,
        req: &ChatRequest,
        on_text: &mut (dyn FnMut(String) + Send + '_),
        on_thinking: &mut (dyn FnMut(String) + Send + '_),
        cancel: CancellationToken,
    ) -> Result<AssistantTurn, LlmError>;
}
