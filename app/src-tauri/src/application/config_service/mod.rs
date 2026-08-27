//! ConfigService：模型 / MCP / 权限规则配置编排。

mod bo;
mod cmd;
mod service;

pub use bo::{McpServerBO, ModelBO, PermRuleBO};
pub use cmd::{
    DeleteMcpCmd, DeleteModelCmd, DeletePermRuleCmd, SaveMcpCmd, SaveModelCmd, SavePermRuleCmd,
    SetDefaultModelCmd, ToggleMcpCmd,
};
pub use service::{ConfigService, ConfigServiceImpl};
