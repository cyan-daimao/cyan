//! 插件相关 Request / DTO。

use serde::{Deserialize, Serialize};

use crate::application::plugin_service::{
    DeletePluginCmd, InstallFromGithubCmd, InstallPluginCmd, MarketItemBO, PluginBO,
    SearchMarketplaceQuery, TogglePluginCmd,
};
use crate::infra::db::fmt_time;

/// install_plugin 请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallPluginRequest {
    /// 插件源路径（zip 文件或目录）
    pub source_path: String,
}

impl From<InstallPluginRequest> for InstallPluginCmd {
    fn from(r: InstallPluginRequest) -> Self {
        Self {
            source_path: r.source_path,
        }
    }
}

/// toggle_plugin 请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TogglePluginRequest {
    /// 插件 id
    pub id: i64,
    /// 启用/禁用
    pub enable: bool,
}

impl From<TogglePluginRequest> for TogglePluginCmd {
    fn from(r: TogglePluginRequest) -> Self {
        Self {
            id: r.id,
            enable: r.enable,
        }
    }
}

/// delete_plugin 请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletePluginRequest {
    /// 插件 id
    pub id: i64,
}

impl From<DeletePluginRequest> for DeletePluginCmd {
    fn from(r: DeletePluginRequest) -> Self {
        Self { id: r.id }
    }
}

/// search_marketplace 请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchMarketplaceRequest {
    /// 关键字（空串 = 全部）
    pub keyword: String,
}

impl From<SearchMarketplaceRequest> for SearchMarketplaceQuery {
    fn from(r: SearchMarketplaceRequest) -> Self {
        Self {
            keyword: r.keyword,
        }
    }
}

/// install_plugin_from_github 请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallPluginFromGithubRequest {
    /// 仓库全名（owner/repo）
    pub full_name: String,
}

impl From<InstallPluginFromGithubRequest> for InstallFromGithubCmd {
    fn from(r: InstallPluginFromGithubRequest) -> Self {
        Self {
            full_name: r.full_name,
        }
    }
}

/// 市场条目 DTO
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketItemDTO {
    /// 仓库全名
    pub full_name: String,
    /// 描述
    pub description: Option<String>,
    /// star 数
    pub stars: i64,
    /// 作者
    pub author: String,
    /// 仓库页面 URL
    pub url: String,
}

impl From<MarketItemBO> for MarketItemDTO {
    fn from(bo: MarketItemBO) -> Self {
        Self {
            full_name: bo.full_name,
            description: bo.description,
            stars: bo.stars,
            author: bo.author,
            url: bo.url,
        }
    }
}

/// 插件 DTO
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginDTO {
    /// 插件 id
    pub id: i64,
    /// 插件名
    pub name: String,
    /// 版本
    pub version: String,
    /// 作者
    pub author: String,
    /// 描述
    pub description: String,
    /// 状态（enabled/disabled）
    pub status: String,
    /// 携带技能数
    pub skill_count: i64,
    /// 携带 MCP 服务器数
    pub mcp_count: i64,
    /// 携带权限规则数
    pub rule_count: i64,
    /// 安装时间（YYYY-MM-DD HH:MM:SS）
    pub installed_at: String,
}

impl From<PluginBO> for PluginDTO {
    fn from(bo: PluginBO) -> Self {
        Self {
            id: bo.id,
            name: bo.name,
            version: bo.version,
            author: bo.author,
            description: bo.description,
            status: bo.status,
            skill_count: bo.skill_count,
            mcp_count: bo.mcp_count,
            rule_count: bo.rule_count,
            installed_at: fmt_time(&bo.installed_at),
        }
    }
}
