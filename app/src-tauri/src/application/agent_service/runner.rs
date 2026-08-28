//! Agent 运行循环：LLM 流式 → 权限判定 → 工具执行 → checkpoint → ctx/compaction。
//! 所有等待点都选挂 CancellationToken，杜绝悬置（TECH_DESIGN 6.1）。

use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};

use crate::domain::agent::{
    AgentEvent, AgentRun, ApprovalDecision, CancellationToken, ChangeInfo, ChatMessage, ChatRole,
    ChatToolCall, Checkpoint, CheckpointRepository, LlmError, LlmGateway, PermMode,
    PermissionEngine, RunEventSink, RunResult, TodoItem, TokenUsage, ToolCall, ToolExecutor,
    ToolOutput, ToolSpec,
};
use crate::domain::config::{ModelConfig, PermAction, PermRuleRepository, PermissionRule};
use crate::domain::session::{Message, MessageKind, MessageRepository, Session, SessionRepository, COMPACT_THRESHOLD};
use crate::domain::shared::ProjectPath;
use crate::infra::db::now_local;

/// 单次运行最大 Agent 轮次（防失控）
const MAX_ITERS: usize = 25;

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

/// 系统提示词（含 AGENTS.md 项目指令注入）
fn system_prompt(root: &ProjectPath) -> String {
    let mut prompt = format!(
        "你是 cyan，一个运行在用户桌面的 AI 编程 Agent。项目根目录：{}。\n\
         通过工具完成任务：Read/Grep/Glob 查代码、Edit/MultiEdit/Write 改文件、Bash 执行命令、TodoWrite 维护 TODO、WebFetch 读取公开网页。\n\
         所有文件路径使用相对项目根的相对路径。修改前先 Read 了解上下文。",
        root.root().display()
    );
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
            MessageKind::User => out.push(ChatMessage::text(ChatRole::User, text)),
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

/// 压缩：简化版摘要做截断拼接（不调 LLM），重写消息表并回落 ctx
async fn compact_session(
    ctx: &RunContext,
    session: &mut Session,
    llm_messages: &mut Vec<ChatMessage>,
    root: &ProjectPath,
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
    let summary_text = if parts.is_empty() {
        "【压缩摘要】早期对话已压缩。".to_string()
    } else {
        format!(
            "【压缩摘要】已压缩 {} 条早期消息，要点：\n- {}",
            parts.len(),
            parts.join("\n- ")
        )
    };
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
    // 重建 LLM 上下文
    *llm_messages = vec![ChatMessage::text(ChatRole::System, system_prompt(root))];
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
    let tools: Vec<ToolSpec> = builtin_tools()
        .into_iter()
        .filter(|t| !disabled_tools.iter().any(|d| d == &t.name))
        .collect();
    let mut llm_messages = vec![ChatMessage::text(ChatRole::System, system_prompt(&root))];
    llm_messages.extend(history_to_chat(&session.messages));
    let mut total_usage = TokenUsage::default();
    let mut result = RunResult::Done;
    let mut completed = false;

    'outer: for _ in 0..MAX_ITERS {
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
            tool_calls: turn.tool_calls.clone(),
            tool_call_id: None,
        });

        // ---- 逐个处理工具调用 ----
        for tc in turn.tool_calls {
            let call = tool_call_from(tc);
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
                format!("工具调用被拒绝：{reason}")
            } else {
                ctx.sink.emit(AgentEvent::ToolStart {
                    session_id,
                    call_id: call.call_id.clone(),
                    tool: call.tool.clone(),
                    arg: call.arg.clone(),
                });
                let out: ToolOutput = ctx.executor.execute(&root, &call, cancel.clone()).await;
                ctx.sink.emit(AgentEvent::ToolEnd {
                    session_id,
                    call_id: call.call_id.clone(),
                    status: out.status.as_str().to_string(),
                    output: out.output.clone(),
                    note: out.note.clone(),
                });
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
            let summary = compact_session(&ctx, &mut session, &mut llm_messages, &root).await;
            ctx.sink.emit(AgentEvent::Compacted {
                session_id,
                summary,
            });
        }
    }

    if !completed {
        result = RunResult::Error(format!("超过最大工具轮次（{MAX_ITERS}）"));
    }
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

    #[test]
    fn system_prompt_injects_agents_md() {
        let tmp = tempfile::tempdir().unwrap();
        let root = ProjectPath::new(tmp.path()).unwrap();
        // 无 AGENTS.md：不含项目指令段
        let prompt = system_prompt(&root);
        assert!(!prompt.contains("AGENTS.md"));
        // 存在 AGENTS.md：拼接到末尾
        std::fs::write(tmp.path().join("AGENTS.md"), "提交信息用中文").unwrap();
        let prompt = system_prompt(&root);
        assert!(prompt.contains("项目指令（AGENTS.md，须遵守）：\n提交信息用中文"));
    }

    #[test]
    fn builtin_tools_include_new_tools() {
        let names: Vec<String> = builtin_tools().iter().map(|t| t.name.clone()).collect();
        for expected in ["Grep", "Glob", "WebFetch", "MultiEdit", "Read", "Edit", "Write", "Bash"] {
            assert!(names.iter().any(|n| n == expected), "缺少工具：{expected}");
        }
    }
}
