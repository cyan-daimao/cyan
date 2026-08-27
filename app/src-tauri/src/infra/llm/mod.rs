//! LLM client：OpenAI 兼容 SSE 流式（协议 response 对象不出本层）。

mod client;
mod protocol;

pub use client::OpenAiClient;
