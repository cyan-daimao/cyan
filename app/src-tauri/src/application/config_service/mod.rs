//! ConfigService：模型 / MCP / 权限规则配置编排 + MCP 市场搜索。

mod bo;
mod cmd;
mod service;

pub use bo::{McpMarketItemBO, McpServerBO, ModelBO, PermRuleBO};
pub use cmd::{
    DeleteMcpCmd, DeleteModelCmd, DeletePermRuleCmd, SaveMcpCmd, SaveModelCmd, SavePermRuleCmd,
    SearchMcpMarketQuery, SetDefaultModelCmd, ToggleMcpCmd,
};
pub use service::{ConfigService, ConfigServiceImpl};
