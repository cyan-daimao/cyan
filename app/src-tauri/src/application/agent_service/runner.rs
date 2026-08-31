//! Agent 运行循环：LLM 流式 → 权限判定 → 工具执行 → checkpoint → ctx/compaction。
//! 所有等待点都选挂 CancellationToken，杜绝悬置（TECH_DESIGN 6.1）。

use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};

use crate::domain::agent::{
    AgentEvent, AgentRun, ApprovalDecision, CancellationToken, ChangeInfo, ChatImage, ChatMessage,
    ChatRole, ChatToolCall, Checkpoint, CheckpointRepository, LlmError, LlmGateway, PermMode,
    PermissionEngine, RunEventSink, RunResult, TodoItem, TokenUsage, ToolCall, ToolExecutor,
    ToolOutput, ToolOutputStatus, ToolSpec,
};
use crate::domain::config::{ModelConfig, PermAction, PermRuleRepository, PermissionRule};
use crate::domain::plugin::{PluginRepository, PluginStatus};
use crate::domain::session::{Message, MessageKind, MessageRepository, Session, SessionRepository, COMPACT_THRESHOLD};
use crate::domain::shared::ProjectPath;
use crate::domain::skill::{Skill, SkillSource};
use crate::infra::db::now_local;
use crate::infra::fs::skill as skill_fs;
use crate::infra::mcp::{self, McpGateway};

/// 续跑窗口大小（每 25 轮注入一次续跑提醒；无总轮次上限，cancel 与 ctx≥90% 自动压缩是仅有的安全阀）
const MAX_ITERS: usize = 25;

/// 续跑提醒消息（注入 LLM 上下文，不落库；纯函数便于测试）
fn continuation_prompt(max_iters: usize) -> String {
    format!(
        "【系统提示】你已连续执行 {max_iters} 轮工具调用。请先简要总结当前进度，然后继续完成剩余任务；若任务实际已完成，直接输出最终结论。"
    )
}

/// 续跑用户可见提示（落系统消息 + RunContinued 事件；纯函数便于测试）
fn continuation_note(max_iters: usize, round: i64) -> String {
    format!("⏳ 已执行 {max_iters} 轮工具调用，任务未完成，自动继续执行（第 {round} 次续跑）")
}

/// 连续重复调用中止阈值（同一调用连续 20 次；仅参数完全相同才计数，穿插不同调用即重置）
const DUP_ABORT_AFTER: usize = 20;

/// 连续重复达到该计数后，拦截升级为强提醒（给出可操作引导，给模型缓冲带）
const DUP_WARN_AFTER: usize = 10;

/// 重复调用拦截文案（不执行，结果与上文相同；纯函数便于测试）
fn dup_intercept_text() -> &'static str {
    "⚠️ 重复调用已拦截：该工具以相同参数刚执行过，结果与上文相同。请直接基于已有结果继续；如需不同结果请调整参数。"
}

/// 临近阈值的强提醒（含具体引导，给模型明确出路而非只报错）
fn dup_warn_text(times: usize) -> String {
    format!(
        "🚨 警告：已连续 {times} 次以完全相同参数调用同一工具，再重复将被强制中止。请立即改为：① 调整参数换一种调用方式；② 换用其他工具达成目标；③ 若上文结果已够用，直接基于它继续任务。"
    )
}

/// 循环中止系统提示（纯函数便于测试）
fn dup_abort_note(times: usize) -> String {
    format!(
        "🛑 检测到重复工具调用循环（同一调用连续 {times} 次完全相同），已自动中止。已完成的部分结果仍保留在会话中；可调整任务描述或更换模型后重新发起。"
    )
}

/// 重复调用判定
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DupVerdict {
    /// 正常执行
    Proceed,
    /// 拦截（不执行，回写拦截说明）
    Intercept,
    /// 临近阈值的强提醒（不执行，回写可操作引导）
    Warn,
    /// 中止运行（循环检测）
    Abort,
}

/// 重复工具调用护栏：追踪最近一次调用签名与连续重复次数（内置工具与 mcp__ 统一生效）
#[derive(Default)]
struct DupCallGuard {
    /// 最近一次执行的调用签名（tool, 完整参数序列化）
    last_sig: Option<(String, String)>,
    /// 连续重复次数（首次为 0）
    dup_count: usize,
    /// 最近一次 check 的连续重复次数（含当次；Warn 文案取值用）
    streak: usize,
    /// 同一签名连续执行失败次数（失败后允许原样重试一次，应对网络抖动等瞬时错误）
    fail_streak: usize,
}

impl DupCallGuard {
    /// 判定一次调用：签名与上次相同 → 重复计数 +1；不同 → 清零并更新签名。
    /// 上次同签名执行失败（fail_streak == 1）→ 视为合法重试，计数清零。
    /// 判定阶梯：首次放行 → 第 2~{DUP_WARN_AFTER} 次拦截 → 临近阈值强提醒 → 达阈值中止。
    fn check(&mut self, tool: &str, input: &Value) -> DupVerdict {
        let sig = (tool.to_string(), input.to_string());
        if self.last_sig.as_ref() == Some(&sig) {
            self.streak += 1;
            if self.fail_streak == 1 {
                self.dup_count = 0;
            } else {
                self.dup_count += 1;
            }
        } else {
            self.last_sig = Some(sig);
            self.streak = 1;
            self.dup_count = 0;
            self.fail_streak = 0;
        }
        if self.dup_count >= DUP_ABORT_AFTER - 1 {
            DupVerdict::Abort
        } else if self.dup_count >= DUP_WARN_AFTER {
            DupVerdict::Warn
        } else if self.dup_count >= 1 {
            DupVerdict::Intercept
        } else {
            DupVerdict::Proceed
        }
    }

    /// 最近一次 check 的连续重复次数（含当次；Warn 文案取值用）
    fn last_streak(&self) -> usize {
        self.streak
    }

    /// 回报上次调用的结果：确定性结果（执行成功/被拒绝）重置失败计数；
    /// 执行失败累加（下一次同签名调用按合法重试放行，连续失败重试则重新计入重复）
    fn report_outcome(&mut self, ok: bool) {
        if ok {
            self.fail_streak = 0;
        } else {
            self.fail_streak += 1;
        }
    }
}

/// 审批超时（10 分钟，超时按 reject 处理）
const APPROVAL_TIMEOUT: Duration = Duration::from_secs(600);

/// LLM 可重试错误最大重试次数
const LLM_MAX_RETRY: usize = 3;

/// 运行上下文（依赖注入集合，task 内共享）
#[derive(Clone)]
pub struct RunContext {
    /// 会话仓储
    pub session_repo: Arc<dyn SessionRepository>,
    /// 消息仓储
    pub message_repo: Arc<dyn MessageRepository>,
    /// checkpoint 仓储
    pub checkpoint_repo: Arc<dyn CheckpointRepository>,
    /// 权限规则仓储
    pub perm_repo: Arc<dyn PermRuleRepository>,
    /// LLM 调用端口
    pub llm: Arc<dyn LlmGateway>,
    /// 工具执行端口
    pub executor: Arc<dyn ToolExecutor>,
    /// 事件推送端口
    pub sink: Arc<dyn RunEventSink>,
    /// MCP 连接池端口（工具注入 + 调用路由）
    pub mcp: Arc<dyn McpGateway>,
    /// 插件仓储（启用插件的技能扫描；生产装配传入，测试可用 None）
    pub plugin_repo: Option<Arc<dyn PluginRepository>>,
    /// 插件根目录 `~/.cyan/plugins`（技能扫描用；None 时跳过插件技能）
    pub plugins_dir: Option<std::path::PathBuf>,
    /// 全局技能目录 `~/.cyan/skills`（技能扫描用；None 时跳过全局技能）
    pub skills_dir: Option<std::path::PathBuf>,
}

/// 可用技能摘要（系统提示词注入用）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillDigest {
    /// 技能 id（= 文件名，kebab-case）
    pub id: String,
    /// 名称
    pub name: String,
    /// 描述
    pub description: String,
    /// 技能文件绝对路径（Read/Bash 可读；`~` 开头）
    pub file_path: String,
    /// 来源（global/project/plugin）
    pub source: String,
}

/// 收集可用技能（全局 + 启用插件 + 项目三级合并，同名高优先级覆盖；仅保留 enabled）。
/// 与 SkillService 的合并顺序一致：全局 → 插件（按名排序）→ 项目；各级失败降级为空不阻塞运行。
async fn collect_skills(ctx: &RunContext, root: &ProjectPath) -> Vec<SkillDigest> {
    let mut merged: std::collections::BTreeMap<String, (Skill, String)> = Default::default();
    let mut insert_batch = |skills: Vec<Skill>, dir: &std::path::Path| {
        for s in skills {
            let path = dir.join(format!("{}.md", s.id));
            merged.insert(s.id.clone(), (s, path.to_string_lossy().into_owned()));
        }
    };
    if let Some(dir) = &ctx.skills_dir {
        insert_batch(
            skill_fs::scan_skills(dir, SkillSource::Global).unwrap_or_default(),
            dir,
        );
    }
    if let (Some(repo), Some(plugins_dir)) = (&ctx.plugin_repo, &ctx.plugins_dir) {
        match repo.list().await {
            Ok(plugins) => {
                let mut plugins: Vec<_> =
                    plugins.into_iter().filter(|p| p.status == PluginStatus::Enabled).collect();
                plugins.sort_by(|a, b| a.name.cmp(&b.name));
                for plugin in plugins {
                    let dir = plugins_dir.join(&plugin.name).join("skills");
                    insert_batch(
                        skill_fs::scan_skills(&dir, SkillSource::Plugin(plugin.name.clone()))
                            .unwrap_or_default(),
                        &dir,
                    );
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "技能注入：插件列表查询失败，跳过插件技能");
            }
        }
    }
    if let Ok(project_dir) = skill_fs::project_skills_dir(root) {
        insert_batch(
            skill_fs::scan_skills(&project_dir, SkillSource::Project).unwrap_or_default(),
            &project_dir,
        );
    }
    merged
        .into_values()
        .filter(|(s, _)| s.enabled)
        .map(|(s, path)| SkillDigest {
            id: s.id,
            name: s.name,
            description: s.description,
            file_path: path,
            source: s.source.as_str().to_string(),
        })
        .collect()
}

/// 内置工具表（下发给 LLM 的 JSON Schema）
fn builtin_tools() -> Vec<ToolSpec> {
    let obj = |props: Value, required: &[&str]| {
        json!({
            "type": "object",
            "properties": props,
            "required": required,
        })
    };
    vec![
        ToolSpec {
            name: "Read".into(),
            description: "读取项目内文本文件内容".into(),
            parameters: obj(json!({"path": {"type": "string", "description": "相对项目根的路径"}}), &["path"]),
        },
        ToolSpec {
            name: "Write".into(),
            description: "写入（新建或覆盖）项目内文件".into(),
            parameters: obj(
                json!({
                    "path": {"type": "string"},
                    "content": {"type": "string"},
                }),
                &["path", "content"],
            ),
        },
        ToolSpec {
            name: "Edit".into(),
            description: "对项目内文件做唯一字符串替换（old_string 须唯一匹配）".into(),
            parameters: obj(
                json!({
                    "path": {"type": "string"},
                    "old_string": {"type": "string"},
                    "new_string": {"type": "string"},
                }),
                &["path", "old_string", "new_string"],
            ),
        },
        ToolSpec {
            name: "MultiEdit".into(),
            description: "对同一文件按顺序应用多处字符串替换，每处 old_string 须唯一匹配；任一失败则整次不写盘".into(),
            parameters: obj(
                json!({
                    "path": {"type": "string"},
                    "edits": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "old_string": {"type": "string"},
                                "new_string": {"type": "string"},
                            },
                            "required": ["old_string", "new_string"],
                        },
                    },
                }),
                &["path", "edits"],
            ),
        },
        ToolSpec {
            name: "Grep".into(),
            description: "在项目内按正则搜索文本行，输出 `相对路径:行号: 行内容`（上限 200 条），自动跳过二进制、>1MB 文件与 .git/node_modules/target；include 为可选 glob（如 *.rs）用于过滤文件名，path 限定子目录".into(),
            parameters: obj(
                json!({
                    "pattern": {"type": "string", "description": "正则表达式"},
                    "include": {"type": "string", "description": "可选 glob，过滤文件（作用于相对路径）"},
                    "path": {"type": "string", "description": "可选子目录（相对项目根）"},
                }),
                &["pattern"],
            ),
        },
        ToolSpec {
            name: "Glob".into(),
            description: "按 glob 模式列出项目内匹配的文件（如 src/**/*.rs），返回相对路径列表（按路径排序，上限 500 条），自动跳过 .git/node_modules/target；path 限定子目录".into(),
            parameters: obj(
                json!({
                    "pattern": {"type": "string", "description": "glob 模式"},
                    "path": {"type": "string", "description": "可选子目录（相对项目根）"},
                }),
                &["pattern"],
            ),
        },
        ToolSpec {
            name: "WebFetch".into(),
            description: "抓取指定 http(s) URL 的文本内容（网络访问：会访问项目外的互联网，仅用于读取公开文档/资料；超时 30s，HTML 自动剥离标签，内容截断约 20KB）".into(),
            parameters: obj(
                json!({
                    "url": {"type": "string", "description": "http(s) URL"},
                }),
                &["url"],
            ),
        },
        ToolSpec {
            name: "Bash".into(),
            description: "在项目根目录执行 Bash 命令".into(),
            parameters: obj(
                json!({
                    "command": {"type": "string"},
                    "timeout_secs": {"type": "integer"},
                }),
                &["command"],
            ),
        },
        ToolSpec {
            name: "TodoWrite".into(),
            description: "更新任务 TODO 列表".into(),
            parameters: obj(
                json!({
                    "todos": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": {"type": "integer"},
                                "content": {"type": "string"},
                                "status": {"type": "string", "enum": ["pending", "in_progress", "done"]},
                            },
                        },
                    },
                }),
                &["todos"],
            ),
        },
    ]
}

/// 系统提示词（含 AGENTS.md 项目指令、已连接 MCP 工具、可用技能注入）。
/// 注入内容在每次 run 开始与 compaction 重建时实时收集，新会话/新任务自动生效。
fn system_prompt(
    root: &ProjectPath,
    skills: &[SkillDigest],
    mcp_tools: &[(String, mcp::McpTool)],
) -> String {
    let mut prompt = format!(
        "你是 cyan，一个运行在用户桌面的 AI 编程 Agent。项目根目录：{}。\n\
         通过工具完成任务：Read/Grep/Glob 查代码、Edit/MultiEdit/Write 改文件、Bash 执行命令、TodoWrite 维护 TODO、WebFetch 读取公开网页。\n\
         所有文件路径使用相对项目根的相对路径；需要查看宿主目录内容（技能正文、插件文件、日志等）时，Read/Grep/Glob 允许访问 `~/.cyan`（只读，路径可写 `~/.cyan/...` 或绝对路径）。修改前先 Read 了解上下文。",
        root.root().display()
    );
    // 已连接 MCP 工具清单：模型能看到工具 schema，这里补充职责说明引导选择
    if !mcp_tools.is_empty() {
        let mut lines: Vec<String> = Vec::new();
        for (server, t) in mcp_tools {
            let desc = truncate(&t.description, 120);
            lines.push(format!("- {}: {}", mcp::tool_name(server, &t.name), desc));
        }
        prompt.push_str(&format!(
            "\n\n已连接的 MCP 工具（名称前缀 mcp__<server>__<tool>，可直接调用）：\n{}",
            lines.join("\n")
        ));
    }
    // 可用技能清单：模型按需 Read 技能文件正文并按其指令执行
    if !skills.is_empty() {
        let mut lines: Vec<String> = Vec::new();
        for s in skills {
            lines.push(format!(
                "- {}（{}）：{}；正文：Read {}",
                s.id, s.source, s.description, s.file_path
            ));
        }
        prompt.push_str(&format!(
            "\n\n可用技能（匹配用户需求时，先 Read 技能文件正文，再按其中指令完成任务）：\n{}",
            lines.join("\n")
        ));
    }
    // AGENTS.md 项目指令注入（不存在则静默跳过，截断 8KB 由 infra/fs 保证）
    if let Some(agents) = crate::infra::fs::read_agents_md(root) {
        if !agents.trim().is_empty() {
            prompt.push_str(&format!("\n\n项目指令（AGENTS.md，须遵守）：\n{agents}"));
        }
    }
    prompt
}

/// 持久化一条消息（序号由 Session::append_message 保证自洽）
async fn persist_message(
    ctx: &RunContext,
    session: &mut Session,
    kind: MessageKind,
    payload: String,
) -> Option<Message> {
    let mut message = session.append_message(Message::new(session.id, kind, payload, now_local())).clone();
    match ctx.message_repo.insert(&mut message).await {
        Ok(()) => Some(message),
        Err(e) => {
            tracing::error!(error = %e, "消息落库失败");
            None
        }
    }
}

/// 历史消息 → LLM 上下文消息（payload JSON 为内容本身，非层间转换）
fn history_to_chat(messages: &[Message]) -> Vec<ChatMessage> {
    let mut out = Vec::new();
    for m in messages {
        let v: Value = serde_json::from_str(&m.payload).unwrap_or(Value::Null);
        let text = v
            .get("text")
            .and_then(|t| t.as_str())
            .unwrap_or_default()
            .to_string();
        match m.kind {
            MessageKind::User => {
                // 多模态：payload.images 存在时携带内嵌图片（base64 data URL 在协议层组装）
                let images = v
                    .get("images")
                    .and_then(Value::as_array)
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|img| {
                                Some(ChatImage {
                                    mime: img.get("mime")?.as_str()?.to_string(),
                                    data: img.get("data")?.as_str()?.to_string(),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                out.push(ChatMessage {
                    role: ChatRole::User,
                    content: text,
                    images,
                    tool_calls: Vec::new(),
                    tool_call_id: None,
                });
            }
            MessageKind::Assistant => {
                let tool_calls = v
                    .get("toolCalls")
                    .and_then(|t| t.as_array())
                    .map(|arr| {
                        arr.iter()
                            .map(|c| ChatToolCall {
                                id: c.get("callId").and_then(|x| x.as_str()).unwrap_or_default().to_string(),
                                name: c.get("tool").and_then(|x| x.as_str()).unwrap_or_default().to_string(),
                                arguments: c.get("arguments").and_then(|x| x.as_str()).unwrap_or("{}").to_string(),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                out.push(ChatMessage {
                    role: ChatRole::Assistant,
                    content: text,
                    images: Vec::new(),
                    tool_calls,
                    tool_call_id: None,
                });
            }
            MessageKind::Tool => {
                let output = v
                    .get("output")
                    .and_then(|t| t.as_str())
                    .unwrap_or_default()
                    .to_string();
                out.push(ChatMessage {
                    role: ChatRole::Tool,
                    content: output,
                    images: Vec::new(),
                    tool_calls: Vec::new(),
                    tool_call_id: v
                        .get("callId")
                        .and_then(|t| t.as_str())
                        .map(String::from),
                });
            }
            // compaction 摘要以 user 角色注入，兼容只认单 system 的提供商
            MessageKind::System => out.push(ChatMessage::text(ChatRole::User, text)),
            MessageKind::Approval => continue,
        }
    }
    out
}

/// ChatToolCall → ToolCall（arg 取主参数用于展示与权限匹配）
fn tool_call_from(tc: ChatToolCall) -> ToolCall {
    let input: Value = serde_json::from_str(&tc.arguments).unwrap_or(json!({}));
    let arg = input
        .get("path")
        .or_else(|| input.get("command"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    ToolCall {
        call_id: tc.id,
        tool: tc.name,
        arg,
        input,
    }
}

/// 无 usage 返回时的粗略估算（4 字符 ≈ 1 token）
fn estimate_usage(messages: &[ChatMessage], turn_text: &str) -> TokenUsage {
    let chars: usize = messages.iter().map(|m| m.content.len()).sum();
    TokenUsage {
        input: (chars / 4) as i64,
        output: (turn_text.len() / 4) as i64,
    }
}

/// 等待审批决断：超时 10 分钟按 reject；cancel/中断按 abort
async fn wait_decision(
    rx: tokio::sync::oneshot::Receiver<ApprovalDecision>,
    cancel: CancellationToken,
) -> ApprovalDecision {
    let fut = async {
        tokio::select! {
            _ = cancel.cancelled() => ApprovalDecision::Abort,
            r = rx => r.unwrap_or(ApprovalDecision::Abort),
        }
    };
    tokio::time::timeout(APPROVAL_TIMEOUT, fut)
        .await
        .unwrap_or(ApprovalDecision::Reject)
}

/// 截断文本到 n 字符
fn truncate(s: &str, n: usize) -> String {
    let t: String = s.chars().take(n).collect();
    if s.chars().count() > n {
        format!("{t}…")
    } else {
        t
    }
}

/// 截断拼接的兜底摘要（LLM 失败/取消时回落）
fn fallback_summary(parts: &[String]) -> String {
    if parts.is_empty() {
        "早期对话已压缩。".to_string()
    } else {
        format!("已压缩 {} 条早期消息，要点：\n- {}", parts.len(), parts.join("\n- "))
    }
}

/// 压缩摘要的 LLM 请求消息（纯函数，便于测试）：中文要点式摘要，保留关键决策/TODO/文件路径/错误结论
fn build_compact_messages(parts: &[String]) -> Vec<ChatMessage> {
    let joined = parts.join("\n");
    vec![ChatMessage::text(
        ChatRole::User,
        format!(
            "以下是一段编程助手会话中待压缩的早期消息。请用中文输出要点式摘要（不超过 10 条要点），\
             必须保留：用户的原始诉求、已做出的关键决策、TODO 事项、涉及的文件路径、错误与结论。\
             直接输出要点列表，不要寒暄、不要解释。\n\n{joined}"
        ),
    )]
}

/// 调 LLM 生成摘要：收集流但不触发任何对外回调（不推给前端）；失败/取消返回 None 走兜底
async fn llm_summarize(
    llm: &Arc<dyn LlmGateway>,
    model: &ModelConfig,
    api_key: &str,
    parts: &[String],
    cancel: CancellationToken,
) -> Option<String> {
    let req = crate::domain::agent::ChatRequest {
        base_url: model.base_url.clone(),
        api_key: api_key.to_string(),
        model: model.name.clone(),
        messages: build_compact_messages(parts),
        tools: Vec::new(),
        max_tokens: Some(1024),
    };
    // 收集流但不对外推送：两个独立的空回调（文本与思考都吞掉）
    let mut noop_text = |_: String| {};
    let mut noop_thinking = |_: String| {};
    match llm
        .stream_chat(&req, &mut noop_text, &mut noop_thinking, cancel)
        .await
    {
        Ok(turn) if !turn.text.trim().is_empty() => Some(turn.text.trim().to_string()),
        Ok(_) => {
            tracing::warn!("压缩摘要 LLM 返回空，回落截断拼接");
            None
        }
        Err(e) => {
            tracing::warn!(error = %e, "压缩摘要 LLM 调用失败，回落截断拼接");
            None
        }
    }
}

/// 压缩：优先调 LLM 生成摘要（失败回落截断拼接），重写消息表并回落 ctx
async fn compact_session(
    ctx: &RunContext,
    session: &mut Session,
    llm_messages: &mut Vec<ChatMessage>,
    root: &ProjectPath,
    model: &ModelConfig,
    api_key: &str,
    cancel: CancellationToken,
) -> String {
    let total = session.messages.len();
    let tail_start = total.saturating_sub(total * 40 / 100);
    let mut parts: Vec<String> = Vec::new();
    for (i, m) in session.messages.iter().enumerate() {
        if m.kind == MessageKind::User || i >= tail_start {
            continue;
        }
        let v: Value = serde_json::from_str(&m.payload).unwrap_or(Value::Null);
        let piece = m.text().unwrap_or_else(|| {
            let tool = v.get("tool").and_then(|t| t.as_str()).unwrap_or("tool");
            let arg = v.get("arg").and_then(|t| t.as_str()).unwrap_or("");
            format!("[{tool}] {arg}")
        });
        if !piece.is_empty() {
            parts.push(truncate(&piece, 120));
        }
    }
    let body = if parts.is_empty() {
        fallback_summary(&parts)
    } else {
        match llm_summarize(&ctx.llm, model, api_key, &parts, cancel).await {
            Some(s) => s,
            None => fallback_summary(&parts),
        }
    };
    let summary_text = format!("【压缩摘要】{body}");
    session.compact(Message::text_payload(&summary_text), now_local());
    // 重写消息表：软删全部后按新序号插入
    if let Err(e) = ctx.message_repo.soft_delete_by_session(session.id).await {
        tracing::error!(error = %e, "compaction 清理旧消息失败");
    }
    for m in session.messages.iter_mut() {
        m.id = 0;
        if let Err(e) = ctx.message_repo.insert(m).await {
            tracing::error!(error = %e, "compaction 写入新消息失败");
        }
    }
    // ctx 回落并落库
    session.ctx_percent = 40;
    if let Err(e) = ctx.session_repo.update(session).await {
        tracing::error!(error = %e, "compaction 更新会话失败");
    }
    // 重建 LLM 上下文（技能/MCP 清单重新收集，保证压缩后提示词仍是最新）
    let skills = collect_skills(ctx, root).await;
    let mcp_tools = ctx.mcp.connected_tools();
    *llm_messages = vec![ChatMessage::text(
        ChatRole::System,
        system_prompt(root, &skills, &mcp_tools),
    )];
    llm_messages.extend(history_to_chat(&session.messages));
    summary_text
}

/// Agent 运行主循环
#[allow(clippy::too_many_arguments)]
pub async fn run_loop(
    ctx: RunContext,
    run: Arc<AgentRun>,
    mut session: Session,
    root: ProjectPath,
    model: ModelConfig,
    api_key: String,
    rules: Vec<PermissionRule>,
    mode: PermMode,
    disabled_tools: Vec<String>,
) {
    let session_id = session.id;
    let cancel = run.token();
    let mut engine = PermissionEngine::new(rules, mode);
    // 「能力」面板禁用的工具不下发给 LLM（模型看不到也就不会调用）
    let mut tools: Vec<ToolSpec> = builtin_tools()
        .into_iter()
        .filter(|t| !disabled_tools.iter().any(|d| d == &t.name))
        .collect();
    // 注入已连接 MCP server 的工具：`mcp__<server>__<tool>`，schema 原样透传；
    // 清单同时进系统提示词（让模型知道每个工具的用途）
    let mcp_tools = ctx.mcp.connected_tools();
    for (server, t) in &mcp_tools {
        let name = mcp::tool_name(server, &t.name);
        if disabled_tools.iter().any(|d| d == &name) {
            continue;
        }
        tools.push(ToolSpec {
            name,
            description: t.description.clone(),
            parameters: t.input_schema.clone(),
        });
    }
    // 可用技能收集（全局 + 启用插件 + 项目，enabled 过滤；失败降级为空）
    let skills = collect_skills(&ctx, &root).await;
    let mut llm_messages = vec![ChatMessage::text(ChatRole::System, system_prompt(&root, &skills, &mcp_tools))];
    llm_messages.extend(history_to_chat(&session.messages));
    let mut total_usage = TokenUsage::default();
    let mut result = RunResult::Done;
    let mut completed = false;
    // 续跑计数：每 MAX_ITERS 轮注入一次提醒并无限续跑（无总轮次上限）
    let mut iter = 0usize;
    let mut continuations = 0i64;
    // 重复工具调用护栏（弱模型循环刷同一调用时拦截/中止）
    let mut dup_guard = DupCallGuard::default();

    'outer: loop {
        if iter >= MAX_ITERS {
            // 窗口耗尽且未完成：注入提醒 + 用户可见提示 + 事件，无限续跑直到 completed 或 cancel
            continuations += 1;
            iter = 0;
            // 提醒消息只进 LLM 上下文，不落库
            llm_messages.push(ChatMessage::text(ChatRole::User, continuation_prompt(MAX_ITERS)));
            // 用户可见的系统提示消息 + RunContinued 事件
            let note = continuation_note(MAX_ITERS, continuations);
            persist_message(&ctx, &mut session, MessageKind::System, Message::text_payload(&note)).await;
            ctx.sink.emit(AgentEvent::RunContinued {
                session_id,
                round: continuations,
            });
            continue 'outer;
        }
        iter += 1;
        if cancel.is_cancelled() {
            result = RunResult::Aborted;
            completed = true;
            break;
        }

        // ---- LLM 流式调用（超时/5xx/网络错误指数退避重试 3 次）----
        let req = crate::domain::agent::ChatRequest {
            base_url: model.base_url.clone(),
            api_key: api_key.clone(),
            model: model.name.clone(),
            messages: llm_messages.clone(),
            tools: tools.clone(),
            max_tokens: None,
        };
        let mut turn_opt = None;
        for attempt in 0..LLM_MAX_RETRY {
            let sink = ctx.sink.clone();
            let mut on_text = move |delta: String| {
                sink.emit(AgentEvent::TextDelta { session_id, delta });
            };
            let sink_thinking = ctx.sink.clone();
            let mut on_thinking = move |delta: String| {
                sink_thinking.emit(AgentEvent::ThinkingDelta { session_id, delta });
            };
            match ctx
                .llm
                .stream_chat(&req, &mut on_text, &mut on_thinking, cancel.clone())
                .await
            {
                Ok(turn) => {
                    turn_opt = Some(turn);
                    break;
                }
                Err(LlmError::Aborted) => {
                    result = RunResult::Aborted;
                    completed = true;
                    break 'outer;
                }
                Err(e) if e.retryable() && attempt + 1 < LLM_MAX_RETRY => {
                    tracing::warn!(attempt, error = %e, "LLM 调用失败，退避重试");
                    let backoff = Duration::from_secs(1 << attempt);
                    tokio::select! {
                        _ = tokio::time::sleep(backoff) => {}
                        _ = cancel.cancelled() => {
                            result = RunResult::Aborted;
                            completed = true;
                            break 'outer;
                        }
                    }
                }
                Err(e) => {
                    result = RunResult::Error(e.to_string());
                    completed = true;
                    break 'outer;
                }
            }
        }
        let Some(turn) = turn_opt else {
            if matches!(result, RunResult::Done) {
                result = RunResult::Error("LLM 调用未返回结果".into());
                completed = true;
            }
            break;
        };

        // ---- token/ctx 统计 ----
        let usage = turn
            .usage
            .unwrap_or_else(|| estimate_usage(&llm_messages, &turn.text));
        total_usage.input += usage.input;
        total_usage.output += usage.output;
        let ctx_pct = (usage.input * 100 / model.context_window.max(1)).clamp(0, 99);
        session.update_usage(usage.input, usage.output, ctx_pct);
        if let Err(e) = ctx.session_repo.update(&session).await {
            tracing::error!(error = %e, "更新会话 token 统计失败");
        }
        ctx.sink.emit(AgentEvent::CtxUpdate {
            session_id,
            ctx_percent: session.ctx_percent,
            tokens: TokenUsage {
                input: session.input_tokens,
                output: session.output_tokens,
            },
        });

        // ---- 无工具调用 → 正常收尾 ----
        if turn.tool_calls.is_empty() {
            let mut payload = json!({ "text": turn.text });
            if !turn.thinking.is_empty() {
                payload["thinking"] = json!(turn.thinking);
            }
            persist_message(
                &ctx,
                &mut session,
                MessageKind::Assistant,
                payload.to_string(),
            )
            .await;
            completed = true;
            break;
        }

        // ---- 持久化 assistant 消息（含工具调用卡）----
        let calls_payload: Vec<Value> = turn
            .tool_calls
            .iter()
            .map(|tc| {
                json!({
                    "callId": tc.id,
                    "tool": tc.name,
                    "arguments": tc.arguments,
                })
            })
            .collect();
        let mut call_msg_payload = json!({"text": turn.text, "toolCalls": calls_payload});
        if !turn.thinking.is_empty() {
            call_msg_payload["thinking"] = json!(turn.thinking);
        }
        persist_message(
            &ctx,
            &mut session,
            MessageKind::Assistant,
            call_msg_payload.to_string(),
        )
        .await;
        llm_messages.push(ChatMessage {
            role: ChatRole::Assistant,
            content: turn.text.clone(),
            images: Vec::new(),
            tool_calls: turn.tool_calls.clone(),
            tool_call_id: None,
        });

        // ---- 逐个处理工具调用 ----
        for tc in turn.tool_calls {
            let call = tool_call_from(tc);
            // ---- 重复调用护栏：判定在执行/审批之前（重复调用不触发审批弹窗）----
            let verdict = dup_guard.check(&call.tool, &call.input);
            let guard_streak = dup_guard.last_streak();
            match verdict {
                DupVerdict::Abort => {
                    // 连续重复达阈值：落系统提示 + 错误收尾
                    let note = dup_abort_note(DUP_ABORT_AFTER);
                    persist_message(
                        &ctx,
                        &mut session,
                        MessageKind::System,
                        Message::text_payload(&note),
                    )
                    .await;
                    result = RunResult::Error("重复工具调用循环，已自动中止".into());
                    completed = true;
                    break 'outer;
                }
                DupVerdict::Intercept | DupVerdict::Warn => {
                    // 不再真正执行：回写拦截说明/强提醒（照常落库 + ToolStart/ToolEnd 事件）
                    let warned = matches!(verdict, DupVerdict::Warn);
                    let output = if warned {
                        dup_warn_text(guard_streak)
                    } else {
                        dup_intercept_text().to_string()
                    };
                    let note_label = if warned { "重复调用强提醒" } else { "重复调用已拦截" };
                    ctx.sink.emit(AgentEvent::ToolStart {
                        session_id,
                        call_id: call.call_id.clone(),
                        tool: call.tool.clone(),
                        arg: call.arg.clone(),
                    });
                    ctx.sink.emit(AgentEvent::ToolEnd {
                        session_id,
                        call_id: call.call_id.clone(),
                        status: ToolOutputStatus::Ok.as_str().to_string(),
                        output: output.clone(),
                        note: Some(note_label.to_string()),
                    });
                    persist_message(
                        &ctx,
                        &mut session,
                        MessageKind::Tool,
                        json!({
                            "callId": call.call_id,
                            "tool": call.tool,
                            "arg": call.arg,
                            "status": ToolOutputStatus::Ok.as_str(),
                            "output": output,
                            "note": note_label,
                        })
                        .to_string(),
                    )
                    .await;
                    llm_messages.push(ChatMessage {
                        role: ChatRole::Tool,
                        content: output,
                        images: Vec::new(),
                        tool_calls: Vec::new(),
                        tool_call_id: Some(call.call_id),
                    });
                    continue;
                }
                DupVerdict::Proceed => {}
            }
            let decision = engine.decide(&call.tool, &call.arg);
            let mut deny_reason: Option<String> = None;

            match decision.action {
                PermAction::Deny => {
                    deny_reason = Some(decision.reason.clone());
                }
                PermAction::Allow => {}
                PermAction::Ask => {
                    let d = if mode == PermMode::Auto {
                        // 自动模式：无 deny 命中直接放行并透传 auto 决断
                        ctx.sink.emit(AgentEvent::ApprovalResolved {
                            session_id,
                            call_id: call.call_id.clone(),
                            decision: ApprovalDecision::Auto.as_str().to_string(),
                        });
                        persist_message(
                            &ctx,
                            &mut session,
                            MessageKind::Approval,
                            json!({
                                "callId": call.call_id,
                                "tool": call.tool,
                                "arg": call.arg,
                                "reason": decision.reason,
                                "decision": ApprovalDecision::Auto.as_str(),
                            })
                            .to_string(),
                        )
                        .await;
                        ApprovalDecision::Auto
                    } else {
                        ctx.sink.emit(AgentEvent::ApprovalRequired {
                            session_id,
                            call_id: call.call_id.clone(),
                            tool: call.tool.clone(),
                            arg: call.arg.clone(),
                            reason: decision.reason.clone(),
                        });
                        // 先落库 pending 审批消息：用户切换会话再回来时，审批卡能从 DB 还原
                        let pending_msg = persist_message(
                            &ctx,
                            &mut session,
                            MessageKind::Approval,
                            json!({
                                "callId": call.call_id,
                                "tool": call.tool,
                                "arg": call.arg,
                                "reason": decision.reason,
                                "decision": "pending",
                            })
                            .to_string(),
                        )
                        .await;
                        let d = match run.request_approval(&call) {
                            Ok(rx) => wait_decision(rx, cancel.clone()).await,
                            Err(_) => ApprovalDecision::Abort,
                        };
                        ctx.sink.emit(AgentEvent::ApprovalResolved {
                            session_id,
                            call_id: call.call_id.clone(),
                            decision: d.as_str().to_string(),
                        });
                        // 决断后更新同一条消息的载荷（pending → 最终决断）
                        if let Some(m) = pending_msg {
                            let final_payload = json!({
                                "callId": call.call_id,
                                "tool": call.tool,
                                "arg": call.arg,
                                "reason": decision.reason,
                                "decision": d.as_str(),
                            })
                            .to_string();
                            if let Err(e) = ctx.message_repo.update_payload(m.id, &final_payload).await {
                                tracing::error!(error = %e, "审批消息更新失败");
                            }
                        }
                        d
                    };
                    if d == ApprovalDecision::Abort {
                        result = RunResult::Aborted;
                        completed = true;
                        break 'outer;
                    }
                    if d == ApprovalDecision::Always {
                        // 「总是允许」：规则落库由 AgentService::approve 按用户选择的作用域完成；
                        // 这里只让推导规则对当前运行的后续调用即时生效
                        let mut rule = PermissionRule::always_allow_from(&call.tool, &call.arg);
                        rule.project_id = Some(session.project_id);
                        rule.session_id = Some(session_id);
                        engine.add_rule(rule);
                    }
                    if !d.is_allowed() {
                        deny_reason = Some("用户拒绝了该操作".to_string());
                    }
                }
            }

            // ---- 执行或回写拒绝结果 ----
            let llm_result: String = if let Some(reason) = deny_reason {
                ctx.sink.emit(AgentEvent::ToolStart {
                    session_id,
                    call_id: call.call_id.clone(),
                    tool: call.tool.clone(),
                    arg: call.arg.clone(),
                });
                ctx.sink.emit(AgentEvent::ToolEnd {
                    session_id,
                    call_id: call.call_id.clone(),
                    status: "error".to_string(),
                    output: reason.clone(),
                    note: None,
                });
                persist_message(
                    &ctx,
                    &mut session,
                    MessageKind::Tool,
                    json!({
                        "callId": call.call_id,
                        "tool": call.tool,
                        "arg": call.arg,
                        "status": "error",
                        "output": reason,
                    })
                    .to_string(),
                )
                .await;
                // 被拒绝是确定性结果：保留重复计数（原样重发不再打扰用户/规则引擎）
                dup_guard.report_outcome(true);
                format!("工具调用被拒绝：{reason}")
            } else {
                ctx.sink.emit(AgentEvent::ToolStart {
                    session_id,
                    call_id: call.call_id.clone(),
                    tool: call.tool.clone(),
                    arg: call.arg.clone(),
                });
                let out: ToolOutput = if let Some((server, mcp_tool)) = mcp::parse_tool_name(&call.tool)
                {
                    // MCP 工具：路由到对应连接 tools/call；运行中断连 → 错误文本收尾（不 panic）
                    match ctx.mcp.call_tool(&server, &mcp_tool, call.input.clone()).await {
                        Ok(text) => ToolOutput::ok(text),
                        Err(e) => ToolOutput::error(e.to_string()),
                    }
                } else {
                    // Bash 等内置工具：增量输出实时推 ToolDelta（终端式滚动）
                    let sink_delta = ctx.sink.clone();
                    let call_id_delta = call.call_id.clone();
                    let mut on_output = move |delta: String| {
                        sink_delta.emit(AgentEvent::ToolDelta {
                            session_id,
                            call_id: call_id_delta.clone(),
                            delta,
                        });
                    };
                    ctx.executor
                        .execute(&root, &call, cancel.clone(), &mut on_output)
                        .await
                };
                ctx.sink.emit(AgentEvent::ToolEnd {
                    session_id,
                    call_id: call.call_id.clone(),
                    status: out.status.as_str().to_string(),
                    output: out.output.clone(),
                    note: out.note.clone(),
                });
                // 执行失败 -> 失败计数 +1（下次同签名调用按合法重试放行一次）；成功 -> 重置
                dup_guard.report_outcome(out.status == ToolOutputStatus::Ok);
                // checkpoint 落库 + change_add 事件
                if let Some(cp) = &out.checkpoint {
                    let mut checkpoint = Checkpoint::new(
                        session_id,
                        cp.file_path.clone(),
                        cp.git_ref.clone(),
                        cp.add_lines,
                        cp.del_lines,
                        now_local(),
                    );
                    match ctx.checkpoint_repo.insert(&mut checkpoint).await {
                        Ok(()) => ctx.sink.emit(AgentEvent::ChangeAdd {
                            session_id,
                            change: ChangeInfo {
                                change_id: checkpoint.id,
                                file_path: checkpoint.file_path.clone(),
                                add_lines: checkpoint.add_lines,
                                del_lines: checkpoint.del_lines,
                                rolled_back: false,
                            },
                        }),
                        Err(e) => tracing::error!(error = %e, "checkpoint 落库失败"),
                    }
                }
                // TodoWrite → todo_update 事件
                if call.tool == "TodoWrite" {
                    let todos: Vec<TodoItem> = call
                        .input
                        .get("todos")
                        .and_then(|t| t.as_array())
                        .map(|arr| {
                            arr.iter()
                                .map(|t| TodoItem {
                                    id: t.get("id").and_then(Value::as_i64).unwrap_or(0),
                                    content: t
                                        .get("content")
                                        .and_then(|c| c.as_str())
                                        .unwrap_or_default()
                                        .to_string(),
                                    status: t
                                        .get("status")
                                        .and_then(|s| s.as_str())
                                        .unwrap_or("pending")
                                        .to_string(),
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    ctx.sink.emit(AgentEvent::TodoUpdate { session_id, todos });
                }
                persist_message(
                    &ctx,
                    &mut session,
                    MessageKind::Tool,
                    json!({
                        "callId": call.call_id,
                        "tool": call.tool,
                        "arg": call.arg,
                        "status": out.status.as_str(),
                        "output": out.output,
                        "note": out.note,
                    })
                    .to_string(),
                )
                .await;
                out.output
            };
            llm_messages.push(ChatMessage {
                role: ChatRole::Tool,
                content: llm_result,
                images: Vec::new(),
                tool_calls: Vec::new(),
                tool_call_id: Some(call.call_id),
            });

            if cancel.is_cancelled() {
                result = RunResult::Aborted;
                completed = true;
                break 'outer;
            }
        }

        // ---- ctx ≥ 90% → 自动压缩 ----
        if session.should_compact(COMPACT_THRESHOLD) {
            let summary = compact_session(
                &ctx,
                &mut session,
                &mut llm_messages,
                &root,
                &model,
                &api_key,
                cancel.clone(),
            )
            .await;
            ctx.sink.emit(AgentEvent::Compacted {
                session_id,
                summary,
            });
        }
    }

    // 循环只经 completed=true 的分支退出（done/aborted/error 已在循环内赋值）
    debug_assert!(completed);
    run.finish();
    ctx.sink.emit(AgentEvent::RunEnd {
        session_id,
        result,
        usage: total_usage,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::agent::AssistantTurn;

    #[test]
    fn system_prompt_injects_agents_md() {
        let tmp = tempfile::tempdir().unwrap();
        let root = ProjectPath::new(tmp.path()).unwrap();
        // 无 AGENTS.md：不含项目指令段
        let prompt = system_prompt(&root, &[], &[]);
        assert!(!prompt.contains("AGENTS.md"));
        // 存在 AGENTS.md：拼接到末尾
        std::fs::write(tmp.path().join("AGENTS.md"), "提交信息用中文").unwrap();
        let prompt = system_prompt(&root, &[], &[]);
        assert!(prompt.contains("项目指令（AGENTS.md，须遵守）：\n提交信息用中文"));
    }

    #[test]
    fn system_prompt_injects_skills_and_mcp_tools() {
        let tmp = tempfile::tempdir().unwrap();
        let root = ProjectPath::new(tmp.path()).unwrap();
        let skills = vec![SkillDigest {
            id: "weekly-report".into(),
            name: "周报".into(),
            description: "生成周报".into(),
            file_path: "~/.cyan/skills/weekly-report.md".into(),
            source: "global".into(),
        }];
        let mcp_tools = vec![(
            "cyancat".to_string(),
            mcp::McpTool {
                name: "query".into(),
                description: "查询数据库".into(),
                input_schema: serde_json::json!({"type": "object"}),
            },
        )];
        let prompt = system_prompt(&root, &skills, &mcp_tools);
        // 技能清单：id/来源/描述/正文路径
        assert!(prompt.contains("可用技能"));
        assert!(prompt.contains("- weekly-report（global）：生成周报；正文：Read ~/.cyan/skills/weekly-report.md"));
        // MCP 清单：mcp__<server>__<tool> + 描述
        assert!(prompt.contains("已连接的 MCP 工具"));
        assert!(prompt.contains("- mcp__cyancat__query: 查询数据库"));
        // 基础说明含 ~/.cyan 只读白名单提示
        assert!(prompt.contains("~/.cyan"));
        // 空清单时不注入空段落
        let bare = system_prompt(&root, &[], &[]);
        assert!(!bare.contains("可用技能"));
        assert!(!bare.contains("已连接的 MCP 工具"));
    }

    #[test]
    fn builtin_tools_include_new_tools() {
        let names: Vec<String> = builtin_tools().iter().map(|t| t.name.clone()).collect();
        for expected in ["Grep", "Glob", "WebFetch", "MultiEdit", "Read", "Edit", "Write", "Bash", "TodoWrite"] {
            assert!(names.iter().any(|n| n == expected), "缺少工具：{expected}");
        }
    }

    #[test]
    fn build_compact_messages_contains_parts_and_instructions() {
        let parts = vec!["决定用 sqlite 存储".to_string(), "[Edit] src/db.rs".to_string()];
        let msgs = build_compact_messages(&parts);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].role, ChatRole::User);
        assert!(msgs[0].content.contains("要点式摘要"));
        assert!(msgs[0].content.contains("TODO"));
        assert!(msgs[0].content.contains("文件路径"));
        assert!(msgs[0].content.contains("决定用 sqlite 存储"));
        assert!(msgs[0].content.contains("[Edit] src/db.rs"));
    }

    #[test]
    fn fallback_summary_format() {
        assert_eq!(fallback_summary(&[]), "早期对话已压缩。");
        let s = fallback_summary(&["a".into(), "b".into()]);
        assert!(s.contains("已压缩 2 条早期消息"));
        assert!(s.contains("- a\n- b"));
    }

    /// mock LLM：返回固定结果
    struct MockLlm {
        result: Result<AssistantTurn, LlmError>,
    }

    #[async_trait::async_trait]
    impl LlmGateway for MockLlm {
        async fn stream_chat(
            &self,
            _req: &crate::domain::agent::ChatRequest,
            _on_text: &mut (dyn FnMut(String) + Send + '_),
            _on_thinking: &mut (dyn FnMut(String) + Send + '_),
            _cancel: CancellationToken,
        ) -> Result<AssistantTurn, LlmError> {
            match &self.result {
                Ok(t) => Ok(t.clone()),
                Err(e) => Err(LlmError::Client(e.to_string())),
            }
        }
    }

    fn test_model() -> ModelConfig {
        ModelConfig::new(
            "m".into(),
            "p".into(),
            "https://example.com/v1".into(),
            128_000,
            crate::infra::db::now_local(),
        )
    }

    #[tokio::test]
    async fn llm_summarize_success_uses_llm_text() {
        let llm: Arc<dyn LlmGateway> = Arc::new(MockLlm {
            result: Ok(AssistantTurn {
                text: "- 要点一\n- 要点二".into(),
                ..Default::default()
            }),
        });
        let parts = vec!["早期消息".to_string()];
        let out = llm_summarize(&llm, &test_model(), "sk-x", &parts, CancellationToken::new()).await;
        assert_eq!(out.as_deref(), Some("- 要点一\n- 要点二"));
    }

    #[tokio::test]
    async fn llm_summarize_failure_falls_back() {
        // 调用失败 → None（调用方回落截断拼接）
        let llm: Arc<dyn LlmGateway> = Arc::new(MockLlm {
            result: Err(LlmError::Timeout),
        });
        let parts = vec!["早期消息".to_string()];
        let out = llm_summarize(&llm, &test_model(), "sk-x", &parts, CancellationToken::new()).await;
        assert!(out.is_none());
        // 空响应 → None
        let llm: Arc<dyn LlmGateway> = Arc::new(MockLlm {
            result: Ok(AssistantTurn::default()),
        });
        let out = llm_summarize(&llm, &test_model(), "sk-x", &parts, CancellationToken::new()).await;
        assert!(out.is_none());
    }

    // ---- MCP 工具注入与路由（mock gateway + 真 sqlite 仓储跑完整 run_loop）----

    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    use sqlx::SqlitePool;

    use crate::infra::db::checkpoint_repo::CheckpointRepositoryImpl;
    use crate::infra::db::perm_rule_repo::PermRuleRepositoryImpl;
    use crate::infra::db::session_repo::{MessageRepositoryImpl, SessionRepositoryImpl};
    use crate::infra::mcp::{McpError, McpTool};
    use crate::infra::tools::BuiltinToolExecutor;

    /// 记录每次请求工具列表的 LLM：第一轮发 mcp 工具调用，第二轮纯文本收尾
    #[derive(Default)]
    struct SeqLlm {
        seen_tools: Mutex<Vec<Vec<String>>>,
        calls: AtomicUsize,
    }

    #[async_trait::async_trait]
    impl LlmGateway for SeqLlm {
        async fn stream_chat(
            &self,
            req: &crate::domain::agent::ChatRequest,
            _on_text: &mut (dyn FnMut(String) + Send + '_),
            _on_thinking: &mut (dyn FnMut(String) + Send + '_),
            _cancel: CancellationToken,
        ) -> Result<AssistantTurn, LlmError> {
            self.seen_tools
                .lock()
                .unwrap()
                .push(req.tools.iter().map(|t| t.name.clone()).collect());
            let n = self.calls.fetch_add(1, Ordering::SeqCst);
            if n == 0 {
                Ok(AssistantTurn {
                    text: "调用 MCP 工具".into(),
                    tool_calls: vec![ChatToolCall {
                        id: "c1".into(),
                        name: "mcp__fs__echo".into(),
                        arguments: "{\"text\":\"hi\"}".into(),
                    }],
                    ..Default::default()
                })
            } else {
                Ok(AssistantTurn {
                    text: "完成".into(),
                    ..Default::default()
                })
            }
        }
    }

    /// mock MCP gateway：固定工具列表 + 记录路由调用，fail=true 模拟运行中断连
    struct MockMcpGateway {
        tools: Vec<(String, McpTool)>,
        calls: Mutex<Vec<(String, String)>>,
        fail: bool,
    }

    #[async_trait::async_trait]
    impl McpGateway for MockMcpGateway {
        fn connected_tools(&self) -> Vec<(String, McpTool)> {
            self.tools.clone()
        }
        async fn call_tool(&self, server: &str, tool: &str, _args: Value) -> Result<String, McpError> {
            self.calls
                .lock()
                .unwrap()
                .push((server.to_string(), tool.to_string()));
            if self.fail {
                Err(McpError::NotConnected(server.to_string()))
            } else {
                Ok("pong".into())
            }
        }
        async fn connect(
            &self,
            _server: &str,
            _transport: crate::domain::config::McpTransport,
            _command: &str,
            _headers: &std::collections::HashMap<String, String>,
        ) -> Result<usize, McpError> {
            Ok(0)
        }
        async fn disconnect(&self, _server: &str) {}
    }

    /// 记录事件的 sink
    #[derive(Default)]
    struct RecordingSink {
        events: Mutex<Vec<AgentEvent>>,
    }

    impl RunEventSink for RecordingSink {
        fn emit(&self, event: AgentEvent) {
            self.events.lock().unwrap().push(event);
        }
    }

    fn mock_mcp(fail: bool) -> Arc<MockMcpGateway> {
        Arc::new(MockMcpGateway {
            tools: vec![(
                "fs".into(),
                McpTool {
                    name: "echo".into(),
                    description: "回显".into(),
                    input_schema: json!({"type": "object", "properties": {"text": {"type": "string"}}}),
                },
            )],
            calls: Mutex::new(Vec::new()),
            fail,
        })
    }

    /// 真 sqlite 仓储 + tempdir 项目根，组装 RunContext 与带一条用户消息的会话
    async fn build_ctx(
        pool: &SqlitePool,
        llm: Arc<dyn LlmGateway>,
        sink: Arc<dyn RunEventSink>,
        mcp: Arc<dyn McpGateway>,
    ) -> (RunContext, Session, ProjectPath, tempfile::TempDir) {
        let pid = sqlx::query(
            "INSERT INTO cyan_project (name, path, created_by, updated_by, created_at, updated_at)
             VALUES ('demo', '/tmp/demo', 'local', 'local', '2026-08-27 10:00:00', '2026-08-27 10:00:00')",
        )
        .execute(pool)
        .await
        .unwrap()
        .last_insert_rowid();
        let session_repo: Arc<dyn SessionRepository> =
            Arc::new(SessionRepositoryImpl::new(pool.clone()));
        let message_repo: Arc<dyn MessageRepository> =
            Arc::new(MessageRepositoryImpl::new(pool.clone()));
        let mut session = Session::new(pid, now_local());
        session_repo.insert(&mut session).await.unwrap();
        let mut m = Message::new(
            session.id,
            MessageKind::User,
            Message::text_payload("调用 mcp 工具"),
            now_local(),
        );
        message_repo.insert(&mut m).await.unwrap();
        session.messages = message_repo.list_by_session(session.id).await.unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let root = ProjectPath::new(tmp.path()).unwrap();
        let ctx = RunContext {
            session_repo,
            message_repo,
            checkpoint_repo: Arc::new(CheckpointRepositoryImpl::new(pool.clone())),
            perm_repo: Arc::new(PermRuleRepositoryImpl::new(pool.clone())),
            llm,
            executor: Arc::new(BuiltinToolExecutor),
            sink,
            mcp,
            plugin_repo: None,
            plugins_dir: None,
            skills_dir: None,
        };
        (ctx, session, root, tmp)
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn run_loop_injects_and_routes_mcp_tools(pool: SqlitePool) {
        let mcp = mock_mcp(false);
        let llm = Arc::new(SeqLlm::default());
        let sink = Arc::new(RecordingSink::default());
        let (ctx, session, root, _tmp) =
            build_ctx(&pool, llm.clone(), sink.clone(), mcp.clone()).await;
        let run = Arc::new(AgentRun::new(session.id));
        run.start().unwrap();
        // auto 模式：mcp 工具默认 Ask → 自动放行
        run_loop(ctx, run, session, root, test_model(), "sk-x".into(), vec![], PermMode::Auto, vec![])
            .await;

        // connected server 的工具以 mcp__ 前缀出现在 LLM 请求工具列表
        let seen = llm.seen_tools.lock().unwrap();
        assert!(
            seen[0].iter().any(|n| n == "mcp__fs__echo"),
            "首轮请求应注入 mcp__fs__echo，实际：{:?}",
            seen[0]
        );
        // 调用路由到正确 server/tool
        assert_eq!(
            mcp.calls.lock().unwrap().as_slice(),
            &[("fs".to_string(), "echo".to_string())]
        );
        // 工具结果回传 + 运行正常收尾
        let events = sink.events.lock().unwrap();
        assert!(events.iter().any(|e| matches!(
            e,
            AgentEvent::ToolEnd { status, output, .. } if status == "ok" && output == "pong"
        )));
        assert!(events
            .iter()
            .any(|e| matches!(e, AgentEvent::RunEnd { result: RunResult::Done, .. })));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn run_loop_mcp_disconnect_returns_error_text(pool: SqlitePool) {
        let mcp = mock_mcp(true); // 模拟运行期间连接断开
        let llm = Arc::new(SeqLlm::default());
        let sink = Arc::new(RecordingSink::default());
        let (ctx, session, root, _tmp) =
            build_ctx(&pool, llm.clone(), sink.clone(), mcp.clone()).await;
        let run = Arc::new(AgentRun::new(session.id));
        run.start().unwrap();
        run_loop(ctx, run, session, root, test_model(), "sk-x".into(), vec![], PermMode::Auto, vec![])
            .await;

        let events = sink.events.lock().unwrap();
        // 工具结果返回错误文本（不 panic），运行继续正常收尾
        assert!(events.iter().any(|e| matches!(
            e,
            AgentEvent::ToolEnd { status, output, .. } if status == "error" && output.contains("未连接")
        )));
        assert!(events
            .iter()
            .any(|e| matches!(e, AgentEvent::RunEnd { result: RunResult::Done, .. })));
    }

    // ---- 自动续跑：每 25 轮注入提醒，无轮次上限跑到完成 ----

    /// 前 N 次调用返回工具调用（参数逐轮变化，避开重复调用护栏）、之后返回纯文本完成的 LLM
    struct FiniteToolCallLlm {
        /// 返回工具调用的轮数（之后完成）
        tool_call_rounds: usize,
        calls: AtomicUsize,
        saw_continuation: std::sync::atomic::AtomicBool,
    }

    #[async_trait::async_trait]
    impl LlmGateway for FiniteToolCallLlm {
        async fn stream_chat(
            &self,
            req: &crate::domain::agent::ChatRequest,
            _on_text: &mut (dyn FnMut(String) + Send + '_),
            _on_thinking: &mut (dyn FnMut(String) + Send + '_),
            _cancel: CancellationToken,
        ) -> Result<AssistantTurn, LlmError> {
            let n = self.calls.fetch_add(1, Ordering::SeqCst);
            if req
                .messages
                .iter()
                .any(|m| m.content.contains("【系统提示】你已连续执行"))
            {
                self.saw_continuation
                    .store(true, Ordering::SeqCst);
            }
            if n < self.tool_call_rounds {
                Ok(AssistantTurn {
                    tool_calls: vec![ChatToolCall {
                        id: format!("c{n}"),
                        name: "Read".into(),
                        arguments: format!("{{\"path\":\"f{n}.txt\"}}"),
                    }],
                    ..Default::default()
                })
            } else {
                Ok(AssistantTurn {
                    text: "任务完成".into(),
                    ..Default::default()
                })
            }
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn run_loop_continues_unbounded_until_done(pool: SqlitePool) {
        // 前 100 轮都返回工具调用（跨 4 个 25 轮窗口），第 101 轮完成
        let llm = Arc::new(FiniteToolCallLlm {
            tool_call_rounds: 100,
            calls: AtomicUsize::new(0),
            saw_continuation: std::sync::atomic::AtomicBool::new(false),
        });
        let sink = Arc::new(RecordingSink::default());
        let mcp: Arc<dyn McpGateway> = Arc::new(MockMcpGateway {
            tools: vec![],
            calls: Mutex::new(vec![]),
            fail: false,
        });
        let (ctx, session, root, _tmp) = build_ctx(&pool, llm.clone(), sink.clone(), mcp).await;
        let session_id = session.id;
        let run = Arc::new(AgentRun::new(session_id));
        run.start().unwrap();
        let model = ModelConfig::new("m".into(), "p".into(), "https://x.dev".into(), 128_000, now_local());
        run_loop(ctx, run, session, root, model, "sk".into(), vec![], PermMode::Auto, vec![]).await;

        // 无轮次上限：101 次调用后正常完成（不再出现「超过最大工具轮次」错误）
        assert_eq!(llm.calls.load(Ordering::SeqCst), 101);
        // 续跑提醒已注入 LLM 上下文
        assert!(llm.saw_continuation.load(Ordering::SeqCst));
        // 4 次 RunContinued（100 轮跨 4 个窗口），round 递增（先取出事件再 await，避免持锁跨 await）
        let events: Vec<AgentEvent> = sink.events.lock().unwrap().clone();
        let rounds: Vec<i64> = events
            .iter()
            .filter_map(|e| match e {
                AgentEvent::RunContinued { round, .. } => Some(*round),
                _ => None,
            })
            .collect();
        assert_eq!(rounds, vec![1, 2, 3, 4]);
        // 最终 run_end 为 done，无任何轮次上限错误
        let run_end = events
            .iter()
            .find_map(|e| match e {
                AgentEvent::RunEnd { result, .. } => Some(result.clone()),
                _ => None,
            })
            .expect("应有 run_end");
        assert!(matches!(run_end, RunResult::Done), "无限续跑应跑到 done，实际 {run_end:?}");
        // 用户可见系统提示消息落库 4 条
        let msg_repo = MessageRepositoryImpl::new(pool.clone());
        let msgs = msg_repo.list_by_session(session_id).await.unwrap();
        let notes = msgs
            .iter()
            .filter(|m| {
                m.kind == MessageKind::System
                    && m.text().map(|t| t.contains("自动继续执行")).unwrap_or(false)
            })
            .count();
        assert_eq!(notes, 4);
    }

    #[test]
    fn continuation_prompt_and_note_format() {
        assert!(continuation_prompt(25).contains("已连续执行 25 轮工具调用"));
        let note = continuation_note(25, 3);
        assert!(note.contains("已执行 25 轮"));
        assert!(note.contains("第 3 次续跑"));
    }

    // ---- 重复工具调用护栏 ----

    #[test]
    fn dup_call_guard_verdicts() {
        let mut g = DupCallGuard::default();
        // 首次：正常执行
        assert_eq!(g.check("Read", &json!({"path": "a.rs"})), DupVerdict::Proceed);
        // 第 2 ~ (WARN-1) 次连续相同：拦截
        for _ in 0..(DUP_WARN_AFTER - 1) {
            assert_eq!(g.check("Read", &json!({"path": "a.rs"})), DupVerdict::Intercept);
        }
        // 第 WARN ~ (ABORT-1) 次：强提醒（给模型缓冲带）
        for _ in 0..(DUP_ABORT_AFTER - DUP_WARN_AFTER - 1) {
            assert_eq!(g.check("Read", &json!({"path": "a.rs"})), DupVerdict::Warn);
        }
        // 第 ABORT 次连续相同：中止
        assert_eq!(g.check("Read", &json!({"path": "a.rs"})), DupVerdict::Abort);
        assert_eq!(g.last_streak(), DUP_ABORT_AFTER);
        // 穿插不同调用会重置计数
        let mut g = DupCallGuard::default();
        g.check("Read", &json!({"path": "a.rs"}));
        g.check("Read", &json!({"path": "a.rs"})); // dup 1
        g.check("Read", &json!({"path": "b.rs"})); // 重置
        assert_eq!(g.check("Read", &json!({"path": "b.rs"})), DupVerdict::Intercept);
        // mcp 工具同样生效
        let mut g = DupCallGuard::default();
        g.check("mcp__fs__read", &json!({"path": "x"}));
        assert_eq!(g.check("mcp__fs__read", &json!({"path": "x"})), DupVerdict::Intercept);
    }

    #[test]
    fn dup_guard_full_param_signature_no_false_positive() {
        // 同一文件、不同编辑内容：签名不同，连续执行不误拦（MultiEdit 场景）
        let mut g = DupCallGuard::default();
        assert_eq!(
            g.check("MultiEdit", &json!({"path": "mcp.rs", "edits": [{"old": "a", "new": "b"}]})),
            DupVerdict::Proceed
        );
        assert_eq!(
            g.check("MultiEdit", &json!({"path": "mcp.rs", "edits": [{"old": "c", "new": "d"}]})),
            DupVerdict::Proceed
        );
        assert_eq!(
            g.check("MultiEdit", &json!({"path": "mcp.rs", "edits": [{"old": "e", "new": "f"}]})),
            DupVerdict::Proceed
        );
        // TodoWrite 无 path/command 主参数，旧签名恒为空串必误拦；全量签名按内容区分
        let mut g = DupCallGuard::default();
        assert_eq!(
            g.check("TodoWrite", &json!({"todos": [{"id": 1, "content": "a", "status": "pending"}]})),
            DupVerdict::Proceed
        );
        assert_eq!(
            g.check("TodoWrite", &json!({"todos": [{"id": 1, "content": "a", "status": "in_progress"}]})),
            DupVerdict::Proceed
        );
    }

    #[test]
    fn dup_guard_failed_retry_allowed_once() {
        // 执行失败 -> 原样重试一次放行（网络抖动等瞬时错误）
        let mut g = DupCallGuard::default();
        assert_eq!(g.check("Bash", &json!({"command": "curl x.dev"})), DupVerdict::Proceed);
        g.report_outcome(false); // 失败
        assert_eq!(g.check("Bash", &json!({"command": "curl x.dev"})), DupVerdict::Proceed);
        g.report_outcome(false); // 重试仍失败
        // 第二次原样重试：不再放行
        assert_eq!(g.check("Bash", &json!({"command": "curl x.dev"})), DupVerdict::Intercept);

        // 重试成功后：失败计数清零，再原样调用才是真重复
        let mut g = DupCallGuard::default();
        g.check("Bash", &json!({"command": "curl x.dev"}));
        g.report_outcome(false);
        assert_eq!(g.check("Bash", &json!({"command": "curl x.dev"})), DupVerdict::Proceed);
        g.report_outcome(true); // 重试成功
        assert_eq!(g.check("Bash", &json!({"command": "curl x.dev"})), DupVerdict::Intercept);
    }

    #[test]
    fn dup_guard_texts() {
        assert!(dup_intercept_text().contains("重复调用已拦截"));
        assert!(dup_abort_note(DUP_ABORT_AFTER).contains("连续 20 次"));
        assert!(dup_warn_text(DUP_WARN_AFTER + 1).contains("警告"));
    }

    /// 计数执行器：记录真实执行次数
    #[derive(Default)]
    struct CountingExecutor {
        calls: AtomicUsize,
    }

    #[async_trait::async_trait]
    impl ToolExecutor for CountingExecutor {
        async fn execute(
            &self,
            _project: &ProjectPath,
            _call: &ToolCall,
            _cancel: CancellationToken,
            _on_output: &mut (dyn FnMut(String) + Send + '_),
        ) -> ToolOutput {
            self.calls.fetch_add(1, Ordering::SeqCst);
            ToolOutput::ok("read content")
        }
    }

    /// 手工组装 RunContext（执行器/mcp 可替换；复用真 sqlite 仓储）
    async fn build_guard_ctx(
        pool: &SqlitePool,
        llm: Arc<dyn LlmGateway>,
        executor: Arc<dyn ToolExecutor>,
        sink: Arc<dyn RunEventSink>,
    ) -> (RunContext, Session, ProjectPath, tempfile::TempDir) {
        let pid = sqlx::query(
            "INSERT INTO cyan_project (name, path, created_by, updated_by, created_at, updated_at)
             VALUES ('demo', '/tmp/demo-guard', 'local', 'local', '2026-08-30 10:00:00', '2026-08-30 10:00:00')",
        )
        .execute(pool)
        .await
        .unwrap()
        .last_insert_rowid();
        let session_repo: Arc<dyn SessionRepository> =
            Arc::new(SessionRepositoryImpl::new(pool.clone()));
        let message_repo: Arc<dyn MessageRepository> =
            Arc::new(MessageRepositoryImpl::new(pool.clone()));
        let mut session = Session::new(pid, now_local());
        session_repo.insert(&mut session).await.unwrap();
        let mut m = Message::new(
            session.id,
            MessageKind::User,
            Message::text_payload("读文件"),
            now_local(),
        );
        message_repo.insert(&mut m).await.unwrap();
        session.messages = message_repo.list_by_session(session.id).await.unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let root = ProjectPath::new(tmp.path()).unwrap();
        let mcp: Arc<dyn McpGateway> = Arc::new(MockMcpGateway {
            tools: vec![],
            calls: Mutex::new(vec![]),
            fail: false,
        });
        let ctx = RunContext {
            session_repo,
            message_repo,
            checkpoint_repo: Arc::new(CheckpointRepositoryImpl::new(pool.clone())),
            perm_repo: Arc::new(PermRuleRepositoryImpl::new(pool.clone())),
            llm,
            executor,
            sink,
            mcp,
            plugin_repo: None,
            plugins_dir: None,
            skills_dir: None,
        };
        (ctx, session, root, tmp)
    }

    fn test_model_cfg() -> ModelConfig {
        ModelConfig::new("m".into(), "p".into(), "https://x.dev".into(), 128_000, now_local())
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn dup_loop_intercepts_then_aborts(pool: SqlitePool) {
        // LLM 永远返回同一 Read 调用（弱模型循环）
        let llm = Arc::new(LoopForeverLlm2);
        let executor = Arc::new(CountingExecutor::default());
        let sink = Arc::new(RecordingSink::default());
        let (ctx, session, root, _tmp) =
            build_guard_ctx(&pool, llm, executor.clone(), sink.clone()).await;
        let session_id = session.id;
        let run = Arc::new(AgentRun::new(session_id));
        run.start().unwrap();
        run_loop(ctx, run, session, root, test_model_cfg(), "sk".into(), vec![], PermMode::Auto, vec![]).await;

        // 底层 executor 只被真实调用 1 次（第 2 次起拦截）
        assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
        // 连续 6 次后中止：run_end error + 系统消息落库
        let events: Vec<AgentEvent> = sink.events.lock().unwrap().clone();
        let run_end = events
            .iter()
            .find_map(|e| match e {
                AgentEvent::RunEnd { result, .. } => Some(result.clone()),
                _ => None,
            })
            .expect("应有 run_end");
        match run_end {
            RunResult::Error(m) => assert!(m.contains("重复工具调用循环")),
            other => panic!("应为 Error，实际 {other:?}"),
        }
        let msg_repo = MessageRepositoryImpl::new(pool.clone());
        let msgs = msg_repo.list_by_session(session_id).await.unwrap();
        let sys_notes = msgs
            .iter()
            .filter(|m| {
                m.kind == MessageKind::System
                    && m.text().map(|t| t.contains("重复工具调用循环")).unwrap_or(false)
            })
            .count();
        assert_eq!(sys_notes, 1, "应落一条循环中止系统消息");
        // 拦截文案进入工具消息（第 2~WARN-1 次拦截），WARN 档为强提醒文案
        let intercepted = msgs
            .iter()
            .filter(|m| {
                m.kind == MessageKind::Tool
                    && m.text().is_none()
                    && m.payload.contains("重复调用已拦截")
            })
            .count();
        assert_eq!(intercepted, DUP_WARN_AFTER - 1, "第 2~3 次应被拦截");
        let warned = msgs
            .iter()
            .filter(|m| {
                m.kind == MessageKind::Tool
                    && m.text().is_none()
                    && m.payload.contains("重复调用强提醒")
            })
            .count();
        assert_eq!(
            warned,
            DUP_ABORT_AFTER - DUP_WARN_AFTER - 1,
            "第 WARN~ABORT-1 次应为强提醒"
        );
    }

    /// 永远返回同一 Read 调用的 LLM（护栏测试专用）
    struct LoopForeverLlm2;

    #[async_trait::async_trait]
    impl LlmGateway for LoopForeverLlm2 {
        async fn stream_chat(
            &self,
            _req: &crate::domain::agent::ChatRequest,
            _on_text: &mut (dyn FnMut(String) + Send + '_),
            _on_thinking: &mut (dyn FnMut(String) + Send + '_),
            _cancel: CancellationToken,
        ) -> Result<AssistantTurn, LlmError> {
            Ok(AssistantTurn {
                tool_calls: vec![ChatToolCall {
                    id: "c1".into(),
                    name: "Read".into(),
                    arguments: "{\"path\":\"same.txt\"}".into(),
                }],
                ..Default::default()
            })
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn alternating_calls_reset_dup_count(pool: SqlitePool) {
        // LLM 交替两个不同参数调用 10 轮后完成：计数被重置，不触发拦截/中止
        let llm = Arc::new(AlternatingLlm::default());
        let executor = Arc::new(CountingExecutor::default());
        let sink = Arc::new(RecordingSink::default());
        let (ctx, session, root, _tmp) =
            build_guard_ctx(&pool, llm, executor.clone(), sink.clone()).await;
        let run = Arc::new(AgentRun::new(session.id));
        run.start().unwrap();
        run_loop(ctx, run, session, root, test_model_cfg(), "sk".into(), vec![], PermMode::Auto, vec![]).await;

        // 每次都真实执行（无拦截）
        assert_eq!(executor.calls.load(Ordering::SeqCst), 10);
        let events: Vec<AgentEvent> = sink.events.lock().unwrap().clone();
        let run_end = events
            .iter()
            .find_map(|e| match e {
                AgentEvent::RunEnd { result, .. } => Some(result.clone()),
                _ => None,
            })
            .expect("应有 run_end");
        assert!(matches!(run_end, RunResult::Done), "交替调用应正常完成");
        // 无拦截消息
        let intercepted: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM cyan_message WHERE payload LIKE '%重复调用已拦截%' AND deleted_at IS NULL",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(intercepted.0, 0);
    }

    /// 交替两个参数的 LLM：前 10 轮交替 a.txt/b.txt，之后完成
    #[derive(Default)]
    struct AlternatingLlm {
        calls: AtomicUsize,
    }

    #[async_trait::async_trait]
    impl LlmGateway for AlternatingLlm {
        async fn stream_chat(
            &self,
            _req: &crate::domain::agent::ChatRequest,
            _on_text: &mut (dyn FnMut(String) + Send + '_),
            _on_thinking: &mut (dyn FnMut(String) + Send + '_),
            _cancel: CancellationToken,
        ) -> Result<AssistantTurn, LlmError> {
            let n = self.calls.fetch_add(1, Ordering::SeqCst);
            if n < 10 {
                let path = if n % 2 == 0 { "a.txt" } else { "b.txt" };
                Ok(AssistantTurn {
                    tool_calls: vec![ChatToolCall {
                        id: format!("c{n}"),
                        name: "Read".into(),
                        arguments: format!("{{\"path\":\"{path}\"}}"),
                    }],
                    ..Default::default()
                })
            } else {
                Ok(AssistantTurn {
                    text: "完成".into(),
                    ..Default::default()
                })
            }
        }
    }
}
