//! PermissionRule：用户权限规则（tool + glob pattern → action）。

use chrono::NaiveDateTime;

use crate::domain::DomainError;

/// 权限动作
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermAction {
    /// 直接允许
    Allow,
    /// 需要审批
    Ask,
    /// 拒绝
    Deny,
}

impl PermAction {
    /// 存储字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Ask => "ask",
            Self::Deny => "deny",
        }
    }

    /// 从存储字符串解析
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "allow" => Some(Self::Allow),
            "ask" => Some(Self::Ask),
            "deny" => Some(Self::Deny),
            _ => None,
        }
    }
}

/// 用户权限规则（sort 升序自上而下匹配，首个命中生效）
#[derive(Debug, Clone)]
pub struct PermissionRule {
    /// 主键 id（插入后回填）
    pub id: i64,
    /// 工具名（`*` 表示全部工具）
    pub tool: String,
    /// glob 匹配模式（作用于工具目标：文件路径或 Bash 命令串）
    pub pattern: String,
    /// 动作
    pub action: PermAction,
    /// 匹配顺序（升序）
    pub sort: i64,
    /// 创建时间
    pub created_at: NaiveDateTime,
    /// 更新时间
    pub updated_at: NaiveDateTime,
}

impl PermissionRule {
    /// 校验：工具名/pattern 非空且 pattern 为合法 glob
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.tool.trim().is_empty() {
            return Err(DomainError::Validation("规则工具名不能为空".into()));
        }
        if self.pattern.trim().is_empty() {
            return Err(DomainError::Validation("规则 pattern 不能为空".into()));
        }
        globset::Glob::new(&self.pattern)
            .map_err(|e| DomainError::Validation(format!("非法 glob pattern：{e}")))?;
        Ok(())
    }

    /// 判断规则是否命中（工具名精确匹配或 `*`，目标走 glob）
    pub fn matches(&self, tool: &str, target: &str) -> bool {
        if self.tool != "*" && self.tool != tool {
            return false;
        }
        globset::Glob::new(&self.pattern)
            .map(|g| g.compile_matcher().is_match(target))
            .unwrap_or(false)
    }

    /// 审批「总是允许」自动推导规则 pattern：
    /// 文件类工具取目标所在目录的 `dir/**`；Bash 取命令首词 + ` *`；无目录则用目标本身
    pub fn always_allow_from(tool: &str, target: &str) -> Self {
        let pattern = if tool == "Bash" {
            let first = target.split_whitespace().next().unwrap_or(target);
            format!("{first} *")
        } else {
            match target.rsplit_once('/') {
                Some((dir, _)) if !dir.is_empty() => format!("{dir}/**"),
                _ => target.to_string(),
            }
        };
        let now = chrono::Local::now().naive_local();
        Self {
            id: 0,
            tool: tool.to_string(),
            pattern,
            action: PermAction::Allow,
            sort: 0,
            created_at: now,
            updated_at: now,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn always_allow_file_derives_dir_glob() {
        let r = PermissionRule::always_allow_from("Edit", "src/agent/approval.ts");
        assert_eq!(r.tool, "Edit");
        assert_eq!(r.pattern, "src/agent/**");
        assert_eq!(r.action, PermAction::Allow);
        assert!(r.matches("Edit", "src/agent/other.ts"));
        assert!(!r.matches("Edit", "README.md"));
    }

    #[test]
    fn always_allow_root_file_uses_self() {
        let r = PermissionRule::always_allow_from("Write", "README.md");
        assert_eq!(r.pattern, "README.md");
    }

    #[test]
    fn always_allow_bash_uses_first_token() {
        let r = PermissionRule::always_allow_from("Bash", "cargo build --release");
        assert_eq!(r.pattern, "cargo *");
        assert!(r.matches("Bash", "cargo test"));
        assert!(!r.matches("Bash", "npm test"));
    }

    #[test]
    fn wildcard_tool_matches_all() {
        let now = NaiveDateTime::default();
        let r = PermissionRule {
            id: 0,
            tool: "*".into(),
            pattern: "docs/**".into(),
            action: PermAction::Allow,
            sort: 0,
            created_at: now,
            updated_at: now,
        };
        assert!(r.matches("Read", "docs/a.md"));
        assert!(r.matches("Edit", "docs/a.md"));
        assert!(!r.matches("Read", "src/a.rs"));
    }
}
