//! 插件命令对象。

/// 安装插件命令
#[derive(Debug, Clone)]
pub struct InstallPluginCmd {
    /// 插件源路径（zip 文件或目录）
    pub source_path: String,
}

/// 启停插件命令
#[derive(Debug, Clone)]
pub struct TogglePluginCmd {
    /// 插件 id
    pub id: i64,
    /// true 启用 / false 禁用
    pub enable: bool,
}

/// 卸载插件命令
#[derive(Debug, Clone)]
pub struct DeletePluginCmd {
    /// 插件 id
    pub id: i64,
}

/// 市场搜索查询
#[derive(Debug, Clone)]
pub struct SearchMarketplaceQuery {
    /// 关键字（空串 = 全部 topic:cyan-plugin）
    pub keyword: String,
}

/// 从 GitHub 仓库安装命令
#[derive(Debug, Clone)]
pub struct InstallFromGithubCmd {
    /// 仓库全名（owner/repo）
    pub full_name: String,
}
