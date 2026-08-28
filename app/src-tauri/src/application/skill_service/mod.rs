//! SkillService：技能 list/save/delete（全局 + 项目两级，项目覆盖同名全局）。

mod bo;
mod cmd;
mod service;

pub use bo::SkillBO;
pub use cmd::{
    DeleteSkillCmd, InstallSkillFromGithubCmd, ListSkillQuery, SaveSkillCmd, SearchSkillMarketQuery,
};
pub use service::{SkillService, SkillServiceImpl};
