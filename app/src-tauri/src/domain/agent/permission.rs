//! PermissionEngine：权限判定（内置 deny 优先 → plan 压制 → 用户规则 → 默认值）。

use crate::domain::config::{PermAction, PermissionRule};

use super::tool::is_write_tool;

/// 权限模式（send_task 时指定）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermMode {
    /// 每次写操作都询问
    Ask,
    /// 无 deny 命中时自动批准
    Auto,
    /// 计划模式：压制一切写工具
    Plan,
}

impl PermMode {
    /// 从请求字符串解析（未知值回退 Ask）
    pub fn parse(s: &str) -> Self {
        match s {
            "auto" => Self::Auto,
            "plan" => Self::Plan,
            _ => Self::Ask,
        }
    }
}

/// 审批决断
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalDecision {
    /// 允许一次
    Once,
    /// 总是允许（自动推导规则落库）
    Always,
    /// 拒绝
    Reject,
    /// 中断（interrupt 统一收尾）
    Abort,
    /// 自动批准（auto 模式）
    Auto,
}

impl ApprovalDecision {
    /// 事件/存储字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Once => "once",
            Self::Always => "always",
            Self::Reject => "reject",
            Self::Abort => "abort",
            Self::Auto => "auto",
        }
    }

    /// 从请求字符串解析
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "once" => Some(Self::Once),
            "always" => Some(Self::Always),
            "reject" => Some(Self::Reject),
            _ => None,
        }
    }

    /// 是否为放行决断
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Once | Self::Always | Self::Auto)
    }
}

/// 权限判定结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermDecision {
    /// 动作
    pub action: PermAction,
    /// 判定原因（事件透传给前端展示）
    pub reason: String,
}

/// 权限引擎：decide 判定顺序固定，内置 deny 不可覆盖
pub struct PermissionEngine {
    /// 用户规则（sort 升序）
    rules: Vec<PermissionRule>,
    /// 权限模式
    mode: PermMode,
}

impl PermissionEngine {
    /// 内置 deny glob 清单（basename 维度，不可覆盖）
    const BUILTIN_DENY: &'static [&'static str] = &[".env", ".env.*", "id_rsa", "id_rsa.*"];

    /// 内置危险命令片段
    const BUILTIN_DENY_CMD: &'static [&'static str] =
        &["rm -rf /", "rm -fr /", "rm -rf ~", "mkfs.", ":(){:|:&};:"];

    /// 构造（rules 需已按 sort 升序）
    pub fn new(mut rules: Vec<PermissionRule>, mode: PermMode) -> Self {
        rules.sort_by_key(|r| r.sort);
        Self { rules, mode }
    }

    /// 追加规则（「总是允许」审批后即时生效）
    pub fn add_rule(&mut self, rule: PermissionRule) {
        self.rules.push(rule);
        self.rules.sort_by_key(|r| r.sort);
    }

    /// 判定：1. 内置 deny → 2. plan 压制写工具 → 3. 用户规则首个命中 → 4. 默认（写 Ask / 只读 Allow）
    pub fn decide(&self, tool: &str, target: &str) -> PermDecision {
        // 1. 内置 deny 清单（最高优先级，不可覆盖）
        if Self::matches_builtin_deny(tool, target) {
            return PermDecision {
                action: PermAction::Deny,
                reason: "内置 deny 清单命中，不可覆盖".into(),
            };
        }
        // 2. plan 模式压制一切写工具（连同 allow 规则一起压制）
        if self.mode == PermMode::Plan && is_write_tool(tool) {
            return PermDecision {
                action: PermAction::Deny,
                reason: "plan 模式禁止写操作".into(),
            };
        }
        // 3. 用户规则 sort 升序首个命中
        for rule in &self.rules {
            if rule.matches(tool, target) {
                return PermDecision {
                    action: rule.action,
                    reason: format!("命中用户规则 {} {}", rule.tool, rule.pattern),
                };
            }
        }
        // 4. 默认：写类 Ask，只读 Allow
        if is_write_tool(tool) {
            PermDecision {
                action: PermAction::Ask,
                reason: "写类工具默认需要审批".into()
            }
        } else {
            PermDecision {
                action: PermAction::Allow,
                reason: "只读工具默认允许".into(),
            }
        }
    }

    /// 内置 deny 匹配：敏感文件（basename glob）与危险命令片段
    fn matches_builtin_deny(tool: &str, target: &str) -> bool {
        // 敏感文件：取 basename 匹配 .env* / id_rsa*
        let basename = target.rsplit('/').next().unwrap_or(target);
        let file_hit = Self::BUILTIN_DENY.iter().any(|pat| {
            globset::Glob::new(pat)
                .map(|g| g.compile_matcher().is_match(basename))
                .unwrap_or(false)
        });
        if file_hit {
            return true;
        }
        // 危险命令（仅 Bash）：命令串归一化后包含危险片段
        if tool == "Bash" {
            let normalized: String = target.split_whitespace().collect::<Vec<_>>().join(" ");
            return Self::BUILTIN_DENY_CMD
                .iter()
                .any(|frag| normalized.contains(frag));
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDateTime;

    fn rule(tool: &str, pattern: &str, action: PermAction, sort: i64) -> PermissionRule {
        PermissionRule {
            id: 0,
            tool: tool.into(),
            pattern: pattern.into(),
            action,
            sort,
            created_at: NaiveDateTime::default(),
            updated_at: NaiveDateTime::default(),
        }
    }

    #[test]
    fn builtin_deny_has_top_priority_over_allow_rule() {
        // 即使有 allow 规则命中，.env 仍 Deny
        let rules = vec![rule("Read", "**/.env", PermAction::Allow, 0)];
        let engine = PermissionEngine::new(rules, PermMode::Ask);
        let d = engine.decide("Read", "config/.env");
        assert_eq!(d.action, PermAction::Deny);
        let d = engine.decide("Read", ".env.production");
        assert_eq!(d.action, PermAction::Deny);
        let d = engine.decide("Read", "keys/id_rsa");
        assert_eq!(d.action, PermAction::Deny);
        let d = engine.decide("Read", "keys/id_rsa.pub");
        assert_eq!(d.action, PermAction::Deny);
    }

    #[test]
    fn builtin_deny_dangerous_bash() {
        let engine = PermissionEngine::new(vec![], PermMode::Auto);
        assert_eq!(engine.decide("Bash", "rm -rf /").action, PermAction::Deny);
        assert_eq!(
            engine.decide("Bash", "sudo  rm -rf / --no-preserve-root").action,
            PermAction::Deny
        );
        assert_eq!(engine.decide("Bash", "rm -rf ./target").action, PermAction::Ask);
    }

    #[test]
    fn user_rules_first_match_by_sort_asc() {
        let rules = vec![
            rule("Bash", "cargo *", PermAction::Allow, 10),
            rule("Bash", "cargo *", PermAction::Deny, 1),
        ];
        let engine = PermissionEngine::new(rules, PermMode::Ask);
        // sort=1 的 deny 规则先命中
        assert_eq!(engine.decide("Bash", "cargo build").action, PermAction::Deny);
    }

    #[test]
    fn write_tools_default_ask_readonly_default_allow() {
        let engine = PermissionEngine::new(vec![], PermMode::Ask);
        assert_eq!(engine.decide("Edit", "src/a.rs").action, PermAction::Ask);
        assert_eq!(engine.decide("Write", "src/a.rs").action, PermAction::Ask);
        assert_eq!(engine.decide("Bash", "ls").action, PermAction::Ask);
        assert_eq!(engine.decide("Read", "src/a.rs").action, PermAction::Allow);
        assert_eq!(engine.decide("Glob", "src/**").action, PermAction::Allow);
    }

    #[test]
    fn plan_mode_suppresses_write_even_with_allow_rule() {
        let rules = vec![rule("Edit", "src/**", PermAction::Allow, 0)];
        let engine = PermissionEngine::new(rules, PermMode::Plan);
        assert_eq!(engine.decide("Edit", "src/a.rs").action, PermAction::Deny);
        assert_eq!(engine.decide("Bash", "cargo build").action, PermAction::Deny);
        // 只读工具不受压制
        assert_eq!(engine.decide("Read", "src/a.rs").action, PermAction::Allow);
        // 内置 deny 依旧优先
        assert_eq!(engine.decide("Read", ".env").action, PermAction::Deny);
    }

    #[test]
    fn mcp_tools_default_ask() {
        // MCP 注入工具视为写类，默认 Ask（TECH_DESIGN 6.5）
        let engine = PermissionEngine::new(vec![], PermMode::Ask);
        assert_eq!(engine.decide("mcp__fs__write", "x").action, PermAction::Ask);
    }
}
