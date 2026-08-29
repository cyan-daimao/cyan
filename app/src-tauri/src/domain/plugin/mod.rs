//! 插件域：Plugin 充血对象（manifest 校验、enable/disable 状态迁移）。

#[allow(clippy::module_inception)]
pub mod plugin;
pub mod repository;
pub mod sidecar;

pub use plugin::{BackendDecl, BackendMcpDecl, Plugin, PluginManifest, PluginStatus};
pub use repository::PluginRepository;
pub use sidecar::{SidecarGateway, SidecarInfo, SidecarStatus};
