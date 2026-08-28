//! Skill：技能（一个 Markdown 文件，文件名即 id，kebab-case；PLUGIN_DESIGN 第 2 节）。

use crate::domain::DomainError;

/// 技能来源作用域
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillSource {
    /// 全局（`~/.cyan/skills/`）
    Global,
    /// 项目级（`<项目根>/.cyan/skills/`，同名覆盖全局与插件）
    Project,
    /// 插件携带（`<插件目录>/skills/`，同名覆盖全局）
    Plugin(String),
}

impl SkillSource {
    /// DTO 输出字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Project => "project",
            Self::Plugin(_) => "plugin",
        }
    }

    /// 来源插件名（仅 Plugin 变体）
    pub fn plugin_name(&self) -> Option<String> {
        match self {
            Self::Plugin(name) => Some(name.clone()),
            _ => None,
        }
    }

    /// 从请求字符串解析（仅 global/project 可写）
    pub fn parse(s: &str) -> Result<Self, DomainError> {
        match s {
            "global" => Ok(Self::Global),
            "project" => Ok(Self::Project),
            other => Err(DomainError::Validation(format!("非法技能作用域：{other}"))),
        }
    }
}

/// 技能
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skill {
    /// 技能 id（文件名，kebab-case，不含扩展名）
    pub id: String,
    /// 名称（frontmatter `name`，缺省回退 id）
    pub name: String,
    /// 描述（frontmatter `description`）
    pub description: String,
    /// 是否启用（frontmatter `enabled`，缺省 true）
    pub enabled: bool,
    /// 来源作用域
    pub source: SkillSource,
    /// 市场来源仓库（owner/repo，手动创建为 None）
    pub market_repo: Option<String>,
    /// 正文 prompt 模板（支持 `$ARGUMENTS` 占位符）
    pub content: String,
}

impl Skill {
    /// 校验技能 id / 文件名：kebab-case（小写字母数字 + 连字符），不含路径分隔与扩展名
    pub fn validate_id(id: &str) -> Result<(), DomainError> {
        if id.is_empty() {
            return Err(DomainError::Validation("技能文件名不能为空".into()));
        }
        let kebab = id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        let well_formed = !id.starts_with('-') && !id.ends_with('-') && !id.contains("--");
        if !kebab || !well_formed {
            return Err(DomainError::Validation(format!(
                "技能文件名须为 kebab-case（小写字母/数字/连字符）：{id}"
            )));
        }
        Ok(())
    }

    /// 校验：id 合法、名称非空
    pub fn validate(&self) -> Result<(), DomainError> {
        Self::validate_id(&self.id)?;
        if self.name.trim().is_empty() {
            return Err(DomainError::Validation("技能名称不能为空".into()));
        }
        Ok(())
    }

    /// 展开模板：把 `$ARGUMENTS` 替换为用户参数（无占位符时参数追加到末尾）
    pub fn expand(&self, args: &str) -> String {
        if self.content.contains("$ARGUMENTS") {
            self.content.replace("$ARGUMENTS", args)
        } else if args.trim().is_empty() {
            self.content.clone()
        } else {
            format!("{}\n\n{}", self.content, args)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skill(content: &str) -> Skill {
        Skill {
            id: "weekly-report".into(),
            name: "周报".into(),
            description: "生成周报".into(),
            enabled: true,
            source: SkillSource::Global,
            market_repo: None,
            content: content.into(),
        }
    }

    #[test]
    fn expand_replaces_arguments() {
        let s = skill("汇总以下提交生成周报：\n$ARGUMENTS\n完");
        assert_eq!(s.expand("abc123 fix bug"), "汇总以下提交生成周报：\nabc123 fix bug\n完");
        // 多处占位符全部替换
        let s = skill("$ARGUMENTS 与 $ARGUMENTS");
        assert_eq!(s.expand("x"), "x 与 x");
    }

    #[test]
    fn expand_appends_when_no_placeholder() {
        let s = skill("生成周报");
        assert_eq!(s.expand("本周"), "生成周报\n\n本周");
        assert_eq!(s.expand(""), "生成周报");
    }

    #[test]
    fn validate_id_kebab_case() {
        assert!(Skill::validate_id("weekly-report").is_ok());
        assert!(Skill::validate_id("a").is_ok());
        assert!(Skill::validate_id("a1-b2").is_ok());
        assert!(Skill::validate_id("").is_err());
        assert!(Skill::validate_id("Weekly").is_err());
        assert!(Skill::validate_id("-lead").is_err());
        assert!(Skill::validate_id("tail-").is_err());
        assert!(Skill::validate_id("double--dash").is_err());
        assert!(Skill::validate_id("a/b").is_err());
        assert!(Skill::validate_id("a.md").is_err());
        assert!(Skill::validate_id("..").is_err());
    }

    #[test]
    fn source_roundtrip() {
        assert_eq!(SkillSource::parse("global").unwrap().as_str(), "global");
        assert_eq!(SkillSource::parse("project").unwrap(), SkillSource::Project);
        assert!(SkillSource::parse("sys").is_err());
    }
}
