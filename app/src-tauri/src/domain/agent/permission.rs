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

    /// 内置危险命令类别（展示用描述；判定语义见 matches_dangerous_bash）
    const BUILTIN_DENY_CMD: &'static [&'static str] = &[
        "rm 删除根目录或通配根（/、/*）",
        "rm 删除用户主目录（~、~/*、$HOME）",
        "mkfs 磁盘格式化",
        "fork 炸弹",
    ];

    /// 内置 deny 敏感文件 glob 清单（配置层只读展示用；匹配语义见 matches_builtin_deny）
    pub fn builtin_deny_file_globs() -> &'static [&'static str] {
        Self::BUILTIN_DENY
    }

    /// 内置危险命令类别描述清单（仅对 Bash 目标做语法级判定；展示用）
    pub fn builtin_deny_cmd_labels() -> &'static [&'static str] {
        Self::BUILTIN_DENY_CMD
    }

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

    /// 内置 deny 匹配：敏感文件（basename glob）与危险命令（语法级判定）
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
        // 危险命令（仅 Bash）：shell 语法级解析后按程序判定
        if tool == "Bash" {
            return matches_dangerous_bash(target);
        }
        false
    }
}

/// Bash 危险命令判定：fork 炸弹整体检查 → 按 shell 语法分段 → token 化 → 程序级判定。
/// 相比朴素子串匹配：项目内绝对路径（如 rm -rf /Users/x/proj/temp.txt）不再误报、
/// 引号内字符串（如 echo "rm -rf /"）不再误报；rm -r -f /、rm -rf ~/* 等变体可命中。
fn matches_dangerous_bash(cmd: &str) -> bool {
    // fork 炸弹：整体去空白后检查（分段会破坏该形态，必须先于切分）
    let squeezed: String = cmd.chars().filter(|c| !c.is_whitespace()).collect();
    if squeezed.contains(":(){:|:&};:") {
        return true;
    }
    split_shell_segments(cmd)
        .iter()
        .any(|seg| segment_dangerous(seg))
}

/// 命令按 ; | && || 切段（引号内的分隔符不切；空段丢弃）
fn split_shell_segments(cmd: &str) -> Vec<String> {
    let mut segs = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    for c in cmd.chars() {
        match quote {
            Some(q) => {
                cur.push(c);
                if c == q {
                    quote = None;
                }
            }
            None => match c {
                '"' | '\'' => {
                    quote = Some(c);
                    cur.push(c);
                }
                ';' | '|' | '&' => segs.push(std::mem::take(&mut cur)),
                _ => cur.push(c),
            },
        }
    }
    segs.push(cur);
    segs.into_iter().filter(|s| !s.trim().is_empty()).collect()
}

/// 段 → token（单双引号包裹内容去除引号；与 infra jsonrpc::shell_split 语义一致）
fn tokenize(seg: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut has_token = false;
    for c in seg.chars() {
        match quote {
            Some(q) => {
                if c == q {
                    quote = None;
                } else {
                    cur.push(c);
                }
            }
            None => {
                if c == '"' || c == '\'' {
                    quote = Some(c);
                    has_token = true;
                } else if c.is_whitespace() {
                    if has_token || !cur.is_empty() {
                        out.push(std::mem::take(&mut cur));
                        has_token = false;
                    }
                } else {
                    cur.push(c);
                    has_token = true;
                }
            }
        }
    }
    if has_token || !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// 段级判定：剥离前导赋值与透明前缀后按程序名判定
fn segment_dangerous(seg: &str) -> bool {
    let mut tokens = tokenize(seg);
    // 透明前缀（sudo/doas/nohup/env/command）与前导赋值交替剥离（覆盖 env FOO=1 rm ...）
    loop {
        while tokens
            .first()
            .map(|t| t.contains('=') && !t.starts_with('-'))
            .unwrap_or(false)
        {
            tokens.remove(0);
        }
        match tokens.first().map(String::as_str) {
            Some("sudo") | Some("doas") | Some("nohup") | Some("env") | Some("command") => {
                tokens.remove(0);
                if tokens
                    .first()
                    .map(|t| t.starts_with('-') && t.len() > 1)
                    .unwrap_or(false)
                {
                    tokens.remove(0);
                    if tokens
                        .first()
                        .map(|t| !t.starts_with('-') && t != "rm" && !t.starts_with("mkfs"))
                        .unwrap_or(false)
                    {
                        tokens.remove(0);
                    }
                }
            }
            _ => break,
        }
    }
    match tokens.first().map(String::as_str) {
        Some("rm") => rm_targets_dangerous(&tokens[1..]),
        Some(p) => p == "mkfs" || p.starts_with("mkfs."),
        _ => false,
    }
}

/// rm 参数 → 是否存在危险删除目标（flag 全部跳过；`--` 之后视为目标）
fn rm_targets_dangerous(args: &[String]) -> bool {
    let mut only_targets = false;
    for t in args {
        if only_targets {
            if dangerous_rm_target(t) {
                return true;
            }
            continue;
        }
        if t == "--" {
            only_targets = true;
        } else if t.starts_with("--") {
            continue; // --recursive / --no-preserve-root 等长选项
        } else if t.starts_with('-') && t.len() > 1 {
            continue; // -rf / -r -f 等短选项
        } else if dangerous_rm_target(t) {
            return true;
        }
    }
    false
}

/// 危险删除目标：根目录 / 主目录及其裸通配（尾斜杠归一后精确匹配）
fn dangerous_rm_target(t: &str) -> bool {
    let norm = t.trim().trim_end_matches('/');
    let norm = if norm.is_empty() { "/" } else { norm };
    matches!(
        norm,
        "/" | "~" | "$HOME" | "${HOME}" | "/*" | "~/*" | "$HOME/*" | "${HOME}/*"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDateTime;

    fn rule(tool: &str, pattern: &str, action: PermAction, sort: i64) -> PermissionRule {
        PermissionRule {
            id: 0,
            project_id: None,
            session_id: None,
            tool: tool.into(),
            pattern: pattern.into(),
            action,
            sort,
            plugin_origin: None,
            created_at: NaiveDateTime::default(),
            updated_at: NaiveDateTime::default(),
            deleted_at: None,
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
        // 根目录 / 主目录及其裸通配：全 deny
        for cmd in [
            "rm -rf /",
            "rm -fr /",
            "rm -r -f /",
            "rm --recursive --force /",
            "rm -rf /*",
            "rm -rf ~",
            "rm -rf ~/*",
            "rm -rf $HOME",
            "echo hi; rm -rf /",
            "sudo  rm -rf / --no-preserve-root",
            "FOO=1 rm -rf /",
            "env rm -rf /",
            "nohup rm -rf ~/* &",
        ] {
            assert_eq!(engine.decide("Bash", cmd).action, PermAction::Deny, "应 deny：{cmd}");
        }
        // mkfs / fork 炸弹
        assert_eq!(engine.decide("Bash", "mkfs.ext4 /dev/sda1").action, PermAction::Deny);
        assert_eq!(engine.decide("Bash", ":(){:|:&};:").action, PermAction::Deny);
        // 引号内字符串不是命令（分段切分尊重引号）
        assert_eq!(engine.decide("Bash", "echo \"rm -rf /\"").action, PermAction::Ask);
        assert_eq!(engine.decide("Bash", "echo 'mkfs.ext4'").action, PermAction::Ask);
        // 项目内路径不误报（含绝对路径 / 相对路径 / 裸通配）
        for cmd in [
            "rm -rf /Users/cy/Documents/workspace/cyan/cyan/temp.txt",
            "rm -rf /Users/cy/Documents/workspace/cyan/cyan/*",
            "rm -rf ./target",
            "rm -rf dist build",
            "rm temp.txt",
        ] {
            assert_ne!(engine.decide("Bash", cmd).action, PermAction::Deny, "不应 deny：{cmd}");
        }
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
        assert_eq!(engine.decide("MultiEdit", "src/a.rs").action, PermAction::Ask);
        assert_eq!(engine.decide("Bash", "ls").action, PermAction::Ask);
        assert_eq!(engine.decide("Read", "src/a.rs").action, PermAction::Allow);
        assert_eq!(engine.decide("Grep", "pattern").action, PermAction::Allow);
        assert_eq!(engine.decide("Glob", "src/**").action, PermAction::Allow);
        assert_eq!(engine.decide("WebFetch", "https://x.dev").action, PermAction::Allow);
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
