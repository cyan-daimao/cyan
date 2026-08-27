//! 项目命令对象。

/// 打开项目命令
#[derive(Debug, Clone)]
pub struct OpenProjectCmd {
    /// 项目目录路径
    pub path: String,
}

/// 新建项目命令
#[derive(Debug, Clone)]
pub struct CreateProjectCmd {
    /// 项目名（同时作为目录名）
    pub name: String,
    /// 父目录
    pub parent: String,
    /// 模板（empty/rust/node）
    pub template: String,
    /// 是否初始化 git 仓库
    pub git_init: bool,
}
