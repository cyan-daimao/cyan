//! Request / DTO 定义与 From 转换（serde camelCase 与前端对齐）。

pub mod agent_dto;
pub mod config_dto;
pub mod file_dto;
pub mod plugin_dto;
pub mod project_dto;
pub mod recycle_dto;
pub mod session_dto;
pub mod skill_dto;

pub use agent_dto::{
    AgentEventDTO, ApproveRequest, ChangeDTO, InterruptRequest, RollbackRequest, SendTaskRequest,
    TodoDTO, TokenUsageDTO,
};
pub use config_dto::{
    DeleteMcpRequest, DeleteModelRequest, DeletePermRuleRequest, ListVisibleRulesRequest,
    McpMarketItemDTO, McpServerDTO, ModelDTO, PermRuleDTO, SaveMcpRequest, SaveModelRequest,
    SavePermRuleRequest, SearchMcpMarketRequest, SetDefaultModelRequest, ToggleMcpRequest,
};
pub use file_dto::{FileNodeDTO, FilePreviewDTO, FilePreviewRequest, FileTreeRequest};
pub use plugin_dto::{
    DeletePluginRequest, InstallPluginFromGithubRequest, InstallPluginRequest, MarketItemDTO,
    PluginDTO, SearchMarketplaceRequest, TogglePluginRequest,
};
pub use project_dto::{CreateProjectRequest, OpenProjectRequest, ProjectDTO, RemoveProjectRequest};
pub use recycle_dto::{ProjectRecycleDTO, RecycleBinDTO, RestoreRecycleItemRequest};
pub use session_dto::{
    CreateSessionRequest, DeleteSessionRequest, GetSessionRequest, ListSessionRequest, MessageDTO,
    ProjectTokenUsageDTO, ProjectTokenUsageRequest, RenameSessionRequest, RestoreSessionRequest,
    EditMessageRequest, SessionDTO, SessionSummaryDTO, SetSessionModelRequest,
};
pub use skill_dto::{
    DeleteSkillRequest, InstallSkillFromGithubRequest, ListSkillRequest, SaveSkillRequest,
    SearchSkillMarketRequest, SkillDTO,
};
