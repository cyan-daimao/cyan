//! 项目域：Project、ProjectTemplate（路径校验、脚手架文件清单）。

#[allow(clippy::module_inception)]
pub mod project;
pub mod repository;

pub use project::{Project, ProjectTemplate};
pub use repository::ProjectRepository;
