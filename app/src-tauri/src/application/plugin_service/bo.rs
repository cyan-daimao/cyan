//! 插件业务对象。

use chrono::NaiveDateTime;

use crate::domain::plugin::Plugin;
use crate::infra::plugin::github::MarketItem;

/// 市场条目 BO
#[derive(Debug, Clone)]
pub struct MarketItemBO {
    /// 仓库全名（owner/repo）
    pub full_name: String,
    /// 描述（可空）
    pub description: Option<String>,
    /// star 数
    pub stars: i64,
    /// 作者
    pub author: String,
    /// 仓库页面 URL
    pub url: String,
}

impl From<MarketItem> for MarketItemBO {
    fn from(m: MarketItem) -> Self {
        Self {
            full_name: m.full_name,
            description: m.description,
            stars: m.stars,
            author: m.author,
            url: m.url,
        }
    }
}

/// 插件 BO
#[derive(Debug, Clone)]
pub struct PluginBO {
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
    /// 安装时间
    pub installed_at: NaiveDateTime,
}

impl From<Plugin> for PluginBO {
    fn from(p: Plugin) -> Self {
        Self {
            id: p.id,
            name: p.name,
            version: p.version,
            author: p.author,
            description: p.description,
            status: p.status.as_str().to_string(),
            skill_count: p.skill_count,
            mcp_count: p.mcp_count,
            rule_count: p.rule_count,
            installed_at: p.created_at,
        }
    }
}
