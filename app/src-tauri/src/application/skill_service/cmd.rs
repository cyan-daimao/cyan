//! 技能命令/查询对象。

/// 技能列表查询
#[derive(Debug, Clone)]
pub struct ListSkillQuery {
    /// 项目路径（空串时只列全局）
    pub project_path: String,
}

/// 保存技能命令（按作用域写盘）
#[derive(Debug, Clone)]
pub struct SaveSkillCmd {
    /// 作用域（global/project）
    pub scope: String,
    /// 文件名即技能 id（kebab-case，不含扩展名）
    pub file_name: String,
    /// 名称
    pub name: String,
    /// 描述
    pub description: String,
    /// 是否启用
    pub enabled: bool,
    /// 正文 prompt 模板
    pub content: String,
    /// 项目路径（scope=project 必填）
    pub project_path: Option<String>,
}

/// 删除技能命令
#[derive(Debug, Clone)]
pub struct DeleteSkillCmd {
    /// 作用域（global/project）
    pub scope: String,
    /// 文件名即技能 id
    pub file_name: String,
    /// 项目路径（scope=project 必填）
    pub project_path: Option<String>,
}

/// 技能市场搜索查询
#[derive(Debug, Clone)]
pub struct SearchSkillMarketQuery {
    /// 关键字（空串 = 全部 topic:cyan-skill）
    pub keyword: String,
    /// 市场源（github / gitee）
    pub source: String,
}

/// 从远端仓库安装技能命令
#[derive(Debug, Clone)]
pub struct InstallSkillFromGithubCmd {
    /// 仓库全名（owner/repo）
    pub full_name: String,
    /// 仓库源（github / gitee，缺省 github）
    pub source: String,
}

impl InstallSkillFromGithubCmd {
    /// 是否走 Gitee 源（缺省/未知值都归 GitHub，保持向后兼容）
    pub fn is_gitee(&self) -> bool {
        self.source.eq_ignore_ascii_case("gitee")
    }
}
