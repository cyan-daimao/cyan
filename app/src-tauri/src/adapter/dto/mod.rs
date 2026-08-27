//! Request / DTO 定义与 From 转换（serde camelCase 与前端对齐）。

pub mod agent_dto;
pub mod config_dto;
pub mod file_dto;
pub mod project_dto;
pub mod session_dto;

pub use agent_dto::{
    AgentEventDTO, ApproveRequest, ChangeDTO, InterruptRequest, RollbackRequest, SendTaskRequest,
    TodoDTO, TokenUsageDTO,
};
pub use config_dto::{
    DeleteMcpRequest, DeleteModelRequest, DeletePermRuleRequest, McpServerDTO, ModelDTO,
    PermRuleDTO, SaveMcpRequest, SaveModelRequest, SavePermRuleRequest, SetDefaultModelRequest,
    ToggleMcpRequest,
};
pub use file_dto::{FileNodeDTO, FilePreviewDTO, FilePreviewRequest, FileTreeRequest};
pub use project_dto::{CreateProjectRequest, OpenProjectRequest, ProjectDTO};
pub use session_dto::{
    CreateSessionRequest, DeleteSessionRequest, GetSessionRequest, ListSessionRequest, MessageDTO,
    SessionDTO, SessionSummaryDTO,
};
