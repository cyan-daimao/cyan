//! Agent 域：AgentRun 状态机、权限引擎、审批、工具调用、checkpoint、LLM/执行端口。

pub mod agent_run;
pub mod cancel;
pub mod checkpoint;
pub mod event;
pub mod llm;
pub mod permission;
pub mod tool;

pub use agent_run::{AgentRun, PendingApproval, RunState};
pub use cancel::CancellationToken;
pub use checkpoint::{Checkpoint, CheckpointGateway, CheckpointRepository};
pub use event::{AgentEvent, ChangeInfo, RunEventSink, RunResult, TodoItem, TokenUsage};
pub use llm::{
    AssistantTurn, ChatImage, ChatMessage, ChatRequest, ChatRole, ChatToolCall, LlmError,
    LlmGateway,
};
pub use permission::{ApprovalDecision, PermDecision, PermMode, PermissionEngine};
pub use tool::{
    is_write_tool, CheckpointPayload, ToolCall, ToolExecutor, ToolOutput, ToolOutputStatus,
    ToolSpec,
};
