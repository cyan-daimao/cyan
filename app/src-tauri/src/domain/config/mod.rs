//! 配置域：模型配置、MCP 服务器、权限规则。

pub mod mcp;
pub mod model;
pub mod perm_rule;
pub mod repository;

pub use mcp::{McpServer, McpStatus, McpTransport};
pub use model::{ModelConfig, ModelStatus};
pub use perm_rule::{PermAction, PermissionRule, RuleScope};
pub use repository::{McpRepository, ModelRepository, PermRuleRepository};
