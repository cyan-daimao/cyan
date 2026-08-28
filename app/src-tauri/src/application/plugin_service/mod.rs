//! PluginService：插件安装/启停/卸载（PLUGIN_DESIGN 3.3 生命周期）。

mod bo;
mod cmd;
mod service;

pub use bo::{MarketItemBO, PluginBO};
pub use cmd::{DeletePluginCmd, InstallFromGithubCmd, InstallPluginCmd, SearchMarketplaceQuery, TogglePluginCmd};
pub use service::{PluginService, PluginServiceImpl};
