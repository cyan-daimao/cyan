//! 技能业务对象。

use crate::domain::skill::Skill;

/// 技能 BO
#[derive(Debug, Clone)]
pub struct SkillBO {
    /// 技能 id（文件名，kebab-case）
    pub id: String,
    /// 名称
    pub name: String,
    /// 描述
    pub description: String,
    /// 是否启用
    pub enabled: bool,
    /// 来源（global/project/plugin）
    pub source: String,
    /// 来源插件名（非插件为 None）
    pub plugin_name: Option<String>,
    /// 市场来源仓库（owner/repo，手动创建为 None）
    pub market_repo: Option<String>,
    /// 正文 prompt 模板
    pub content: String,
}

impl From<Skill> for SkillBO {
    fn from(s: Skill) -> Self {
        Self {
            id: s.id,
            name: s.name,
            description: s.description,
            enabled: s.enabled,
            source: s.source.as_str().to_string(),
            plugin_name: s.source.plugin_name(),
            market_repo: s.market_repo,
            content: s.content,
        }
    }
}
