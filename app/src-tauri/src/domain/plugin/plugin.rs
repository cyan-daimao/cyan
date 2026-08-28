//! Plugin：声明式能力包（PLUGIN_DESIGN 3.2 manifest 规范）。

use chrono::NaiveDateTime;

use crate::domain::DomainError;

/// 插件状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginStatus {
    /// 启用（内容物生效：技能入扫描、规则/MCP 已导入）
    Enabled,
    /// 禁用（内容物整体摘除）
    Disabled,
}

impl PluginStatus {
    /// 存储字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Enabled => "enabled",
            Self::Disabled => "disabled",
        }
    }

    /// 从存储字符串解析
    pub fn parse(s: &str) -> Self {
        match s {
            "disabled" => Self::Disabled,
            _ => Self::Enabled,
        }
    }
}

/// 插件 manifest（manifest.json 协议结构；serde 仅用于该文件本身的解析）
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PluginManifest {
    /// 插件名（必填，合法目录名，kebab-case）
    pub name: String,
    /// 版本
    #[serde(default = "default_version")]
    pub version: String,
    /// 作者
    #[serde(default)]
    pub author: String,
    /// 描述
    #[serde(default)]
    pub description: String,
    /// 最低宿主版本（v1 仅记录不强制）
    #[serde(default)]
    pub cyan_min_version: Option<String>,
    /// 声明的权限（白名单：skills/mcp/rules）
    #[serde(default)]
    pub permissions: Vec<String>,
}

fn default_version() -> String {
    "0.1.0".to_string()
}

/// 权限白名单
const ALLOWED_PERMISSIONS: &[&str] = &["skills", "mcp", "rules"];

impl PluginManifest {
    /// 校验：name 必填且为合法目录名（kebab-case），permissions 全在白名单内
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.name.trim().is_empty() {
            return Err(DomainError::Validation("manifest 缺少 name".into()));
        }
        let legal = self
            .name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');
        if !legal || self.name.starts_with(['-', '_']) || self.name.len() > 64 {
            return Err(DomainError::Validation(format!(
                "插件名须为小写字母/数字/连字符/下划线且不以符号开头：{}",
                self.name
            )));
        }
        for p in &self.permissions {
            if !ALLOWED_PERMISSIONS.contains(&p.as_str()) {
                return Err(DomainError::Validation(format!(
                    "未知权限声明：{p}（白名单：{}）",
                    ALLOWED_PERMISSIONS.join("/")
                )));
            }
        }
        Ok(())
    }

    /// 是否声明了某项权限
    pub fn has_permission(&self, perm: &str) -> bool {
        self.permissions.iter().any(|p| p == perm)
    }
}

/// 插件
#[derive(Debug, Clone)]
pub struct Plugin {
    /// 主键 id（插入后回填）
    pub id: i64,
    /// 插件名（唯一，即包目录名）
    pub name: String,
    /// 版本
    pub version: String,
    /// 作者
    pub author: String,
    /// 描述
    pub description: String,
    /// 状态
    pub status: PluginStatus,
    /// 携带技能数
    pub skill_count: i64,
    /// 携带 MCP 服务器数
    pub mcp_count: i64,
    /// 携带权限规则数
    pub rule_count: i64,
    /// 安装时间
    pub created_at: NaiveDateTime,
    /// 更新时间
    pub updated_at: NaiveDateTime,
}

impl Plugin {
    /// 由 manifest 构造（未持久化，id 待回填）
    pub fn from_manifest(m: &PluginManifest, counts: (i64, i64, i64), now: NaiveDateTime) -> Self {
        Self {
            id: 0,
            name: m.name.clone(),
            version: m.version.clone(),
            author: m.author.clone(),
            description: m.description.clone(),
            status: PluginStatus::Enabled,
            skill_count: counts.0,
            mcp_count: counts.1,
            rule_count: counts.2,
            created_at: now,
            updated_at: now,
        }
    }

    /// 启用：disabled → enabled（幂等：已启用直接返回 false 表示无需重导入）
    pub fn enable(&mut self) -> bool {
        if self.status == PluginStatus::Enabled {
            return false;
        }
        self.status = PluginStatus::Enabled;
        true
    }

    /// 禁用：enabled → disabled（幂等）
    pub fn disable(&mut self) -> bool {
        if self.status == PluginStatus::Disabled {
            return false;
        }
        self.status = PluginStatus::Disabled;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(name: &str, perms: Vec<&str>) -> PluginManifest {
        PluginManifest {
            name: name.into(),
            version: "0.1.0".into(),
            author: "tester".into(),
            description: "d".into(),
            cyan_min_version: None,
            permissions: perms.into_iter().map(String::from).collect(),
        }
    }

    #[test]
    fn manifest_validate_ok() {
        assert!(manifest("my-plugin", vec!["skills", "mcp", "rules"]).validate().is_ok());
        assert!(manifest("my_plugin2", vec![]).validate().is_ok());
    }

    #[test]
    fn manifest_validate_rejects_bad_name() {
        assert!(manifest("", vec![]).validate().is_err());
        assert!(manifest("My Plugin", vec![]).validate().is_err());
        assert!(manifest("../evil", vec![]).validate().is_err());
        assert!(manifest("-lead", vec![]).validate().is_err());
    }

    #[test]
    fn manifest_validate_rejects_unknown_permission() {
        let err = manifest("ok-plugin", vec!["fs:read"]).validate().unwrap_err();
        assert!(matches!(err, DomainError::Validation(_)));
    }

    #[test]
    fn enable_disable_idempotent() {
        let m = manifest("p", vec![]);
        let mut p = Plugin::from_manifest(&m, (1, 2, 3), NaiveDateTime::default());
        assert!(!p.enable(), "已启用时 enable 幂等返回 false");
        assert!(p.disable());
        assert!(!p.disable());
        assert_eq!(p.status, PluginStatus::Disabled);
        assert!(p.enable());
        assert_eq!(p.status, PluginStatus::Enabled);
    }
}
