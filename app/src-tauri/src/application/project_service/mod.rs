//! ProjectService：项目注册/脚手架 + 文件树/预览（只读）。

mod bo;
mod cmd;
mod query;
mod service;

pub use bo::{FileNodeBO, FilePreviewBO, ProjectBO};
pub use cmd::{CreateProjectCmd, OpenProjectCmd};
pub use query::{FilePreviewQuery, FileTreeQuery};
pub use service::{ProjectService, ProjectServiceImpl};
