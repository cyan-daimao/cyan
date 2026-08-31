//! 技能相关 Request / DTO。

use serde::{Deserialize, Serialize};

use crate::application::skill_service::{
    DeleteSkillCmd, InstallSkillFromGithubCmd, ListSkillQuery, SaveSkillCmd, SearchSkillMarketQuery,
    SkillBO,
};

/// list_skills 请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListSkillRequest {
    /// 项目路径（空串时只列全局）
    pub project_path: String,
}

impl From<ListSkillRequest> for ListSkillQuery {
    fn from(r: ListSkillRequest) -> Self {
        Self {
            project_path: r.project_path,
        }
    }
}

/// save_skill 请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveSkillRequest {
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

impl From<SaveSkillRequest> for SaveSkillCmd {
    fn from(r: SaveSkillRequest) -> Self {
        Self {
            scope: r.scope,
            file_name: r.file_name,
            name: r.name,
            description: r.description,
            enabled: r.enabled,
            content: r.content,
            project_path: r.project_path,
        }
    }
}

/// delete_skill 请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteSkillRequest {
    /// 作用域（global/project）
    pub scope: String,
    /// 文件名即技能 id
    pub file_name: String,
    /// 项目路径（scope=project 必填）
    pub project_path: Option<String>,
}

impl From<DeleteSkillRequest> for DeleteSkillCmd {
    fn from(r: DeleteSkillRequest) -> Self {
        Self {
            scope: r.scope,
            file_name: r.file_name,
            project_path: r.project_path,
        }
    }
}

/// search_skill_market 请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchSkillMarketRequest {
    /// 关键字（空串 = 全部）
    pub keyword: String,
    /// 市场源（github / gitee，缺省 github）
    #[serde(default)]
    pub source: String,
}

impl From<SearchSkillMarketRequest> for SearchSkillMarketQuery {
    fn from(r: SearchSkillMarketRequest) -> Self {
        Self {
            keyword: r.keyword,
            source: r.source,
        }
    }
}

/// install_skill_from_github 请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallSkillFromGithubRequest {
    /// 仓库全名（owner/repo）
    pub full_name: String,
    /// 仓库源（github / gitee，缺省 github）
    #[serde(default)]
    pub source: String,
}

impl From<InstallSkillFromGithubRequest> for InstallSkillFromGithubCmd {
    fn from(r: InstallSkillFromGithubRequest) -> Self {
        Self {
            full_name: r.full_name,
            source: r.source,
        }
    }
}

/// 技能 DTO
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDTO {
    /// 技能 id
    pub id: String,
    /// 名称
    pub name: String,
    /// 描述
    pub description: String,
    /// 是否启用
    pub enabled: bool,
    /// 来源（global/project/plugin）
    pub source: String,
    /// 来源插件名（非插件为 null）
    pub plugin_name: Option<String>,
    /// 市场来源仓库（owner/repo，手动创建为 null）
    pub market_repo: Option<String>,
    /// 正文 prompt 模板
    pub content: String,
}

impl From<SkillBO> for SkillDTO {
    fn from(bo: SkillBO) -> Self {
        Self {
            id: bo.id,
            name: bo.name,
            description: bo.description,
            enabled: bo.enabled,
            source: bo.source,
            plugin_name: bo.plugin_name,
            market_repo: bo.market_repo,
            content: bo.content,
        }
    }
}
