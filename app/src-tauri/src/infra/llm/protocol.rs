//! OpenAI 兼容协议对象（仅 infra/llm 内部使用，不出层）。

use serde::{Deserialize, Serialize};

/// 请求消息
#[derive(Debug, Serialize)]
pub struct ReqMessage {
    /// 角色
    pub role: String,
    /// 文本内容（tool_calls 消息可为空）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// assistant 工具调用
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ReqToolCall>>,
    /// tool 消息对应调用 id
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

/// 请求侧工具调用
#[derive(Debug, Serialize)]
pub struct ReqToolCall {
    /// 调用 id
    pub id: String,
    /// 类型（固定 function）
    #[serde(rename = "type")]
    pub kind: String,
    /// 函数体
    pub function: ReqFunction,
}

/// 请求侧函数体
#[derive(Debug, Serialize)]
pub struct ReqFunction {
    /// 工具名
    pub name: String,
    /// 参数 JSON 字符串
    pub arguments: String,
}

/// 请求侧工具定义
#[derive(Debug, Serialize)]
pub struct ReqTool {
    /// 类型（固定 function）
    #[serde(rename = "type")]
    pub kind: String,
    /// 函数定义
    pub function: ReqToolFunction,
}

/// 请求侧工具函数定义
#[derive(Debug, Serialize)]
pub struct ReqToolFunction {
    /// 工具名
    pub name: String,
    /// 描述
    pub description: String,
    /// 参数 JSON Schema
    pub parameters: serde_json::Value,
}

/// chat/completions 请求体
#[derive(Debug, Serialize)]
pub struct ChatCompletionsReq {
    /// 模型名
    pub model: String,
    /// 消息列表
    pub messages: Vec<ReqMessage>,
    /// 工具表（为空则不下发）
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ReqTool>,
    /// 流式开关
    pub stream: bool,
    /// 流式用量选项
    pub stream_options: StreamOptions,
}

/// 流式选项（要求末包返回 usage）
#[derive(Debug, Serialize)]
pub struct StreamOptions {
    /// 是否包含 usage
    pub include_usage: bool,
}

/// SSE chunk
#[derive(Debug, Deserialize)]
pub struct ChatChunk {
    /// 增量 choices
    #[serde(default)]
    pub choices: Vec<ChunkChoice>,
    /// token 用量（末包）
    pub usage: Option<ChunkUsage>,
}

/// chunk choice
#[derive(Debug, Deserialize)]
pub struct ChunkChoice {
    /// 增量
    pub delta: Option<ChunkDelta>,
}

/// chunk 增量
#[derive(Debug, Deserialize)]
pub struct ChunkDelta {
    /// 文本增量
    pub content: Option<String>,
    /// 思考增量（推理模型，Moonshot/DeepSeek 风格）
    pub reasoning_content: Option<String>,
    /// 思考增量（部分提供商用 reasoning 字段）
    pub reasoning: Option<String>,
    /// 工具调用增量
    pub tool_calls: Option<Vec<ChunkToolCall>>,
}

/// chunk 工具调用增量
#[derive(Debug, Deserialize)]
pub struct ChunkToolCall {
    /// 序号（聚合键）
    pub index: Option<usize>,
    /// 调用 id（首片携带）
    pub id: Option<String>,
    /// 函数增量
    pub function: Option<ChunkFunction>,
}

/// chunk 函数增量
#[derive(Debug, Deserialize)]
pub struct ChunkFunction {
    /// 工具名（首片携带）
    pub name: Option<String>,
    /// 参数 JSON 片段
    pub arguments: Option<String>,
}

/// chunk usage
#[derive(Debug, Deserialize)]
pub struct ChunkUsage {
    /// 输入 token
    pub prompt_tokens: Option<i64>,
    /// 输出 token
    pub completion_tokens: Option<i64>,
}

/// 错误响应体
#[derive(Debug, Deserialize)]
pub struct ErrorBody {
    /// 错误详情
    pub error: Option<ErrorDetail>,
}

/// 错误详情
#[derive(Debug, Deserialize)]
pub struct ErrorDetail {
    /// 错误信息
    pub message: Option<String>,
}
