//! Agent 相关 Request / DTO（含事件载荷 AgentEventDTO）。

use serde::{Deserialize, Serialize};

use crate::application::agent_service::{ApproveCmd, InterruptCmd, RollbackCmd, StartRunCmd};
use crate::domain::agent::{AgentEvent, RunResult};
use crate::domain::session::MessageImage;

/// send_task 请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendTaskRequest {
    /// 会话 id
    pub session_id: i64,
    /// 任务文本
    pub text: String,
    /// 随消息上传的图片（mime + base64 data；缺省 = 纯文本）
    #[serde(default)]
    pub images: Vec<ImageDTO>,
    /// 模型名（空串使用默认模型）
    pub model: String,
    /// 权限模式（ask/auto/plan）
    pub perm_mode: String,
    /// 禁用的内置工具名（前端「能力」面板配置）
    pub disabled_tools: Option<Vec<String>>,
    /// 跳过用户消息追加（编辑后重新生成场景）
    #[serde(default)]
    pub skip_append: bool,
}

/// 用户消息内嵌图片 DTO（mime + base64 data，不含 data: 前缀）
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageDTO {
    /// MIME 类型（image/png、image/jpeg、image/webp、image/gif）
    pub mime: String,
    /// base64 编码的图片数据
    pub data: String,
}

impl From<SendTaskRequest> for StartRunCmd {
    fn from(r: SendTaskRequest) -> Self {
        Self {
            session_id: r.session_id,
            text: r.text,
            images: r
                .images
                .into_iter()
                .map(|i| MessageImage { mime: i.mime, data: i.data })
                .collect(),
            model: r.model,
            perm_mode: r.perm_mode,
            disabled_tools: r.disabled_tools.unwrap_or_default(),
            skip_append: r.skip_append,
        }
    }
}

/// interrupt_run 请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InterruptRequest {
    /// 会话 id
    pub session_id: i64,
}

impl From<InterruptRequest> for InterruptCmd {
    fn from(r: InterruptRequest) -> Self {
        Self {
            session_id: r.session_id,
        }
    }
}

/// approve 请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApproveRequest {
    /// 会话 id
    pub session_id: i64,
    /// 调用 id
    pub call_id: String,
    /// 决断（once/always/reject）
    pub decision: String,
    /// 「总是允许」规则作用域（global/project/session，缺省 session）
    pub always_scope: Option<String>,
}

impl From<ApproveRequest> for ApproveCmd {
    fn from(r: ApproveRequest) -> Self {
        Self {
            session_id: r.session_id,
            call_id: r.call_id,
            decision: r.decision,
            always_scope: r.always_scope,
        }
    }
}

/// rollback_change 请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RollbackRequest {
    /// 会话 id
    pub session_id: i64,
    /// 变更 id
    pub change_id: i64,
}

impl From<RollbackRequest> for RollbackCmd {
    fn from(r: RollbackRequest) -> Self {
        Self {
            session_id: r.session_id,
            change_id: r.change_id,
        }
    }
}

/// TODO 项 DTO
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TodoDTO {
    /// 序号
    pub id: i64,
    /// 内容
    pub content: String,
    /// 状态
    pub status: String,
}

/// 变更 DTO
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeDTO {
    /// 变更 id
    pub change_id: i64,
    /// 变更文件（相对项目）
    pub file_path: String,
    /// 新增行数
    pub add_lines: i64,
    /// 删除行数
    pub del_lines: i64,
    /// 是否已回滚
    pub rolled_back: bool,
}

/// token 用量 DTO
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsageDTO {
    /// 输入 token
    pub input: i64,
    /// 输出 token
    pub output: i64,
}

/// Agent 事件 DTO（`agent:event` 载荷：type 判别 + 扁平字段）
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum AgentEventDTO {
    /// LLM 流式文本
    TextDelta {
        /// 会话 id
        session_id: i64,
        /// 文本增量
        delta: String,
    },
    /// LLM 流式思考过程（推理模型）
    ThinkingDelta {
        /// 会话 id
        session_id: i64,
        /// 思考增量
        delta: String,
    },
    /// 工具开始执行
    ToolStart {
        /// 会话 id
        session_id: i64,
        /// 调用 id
        call_id: String,
        /// 工具名
        tool: String,
        /// 主参数
        arg: String,
    },
    /// 工具完成/失败
    ToolEnd {
        /// 会话 id
        session_id: i64,
        /// 调用 id
        call_id: String,
        /// 状态
        status: String,
        /// 输出
        output: String,
        /// 备注
        note: Option<String>,
    },
    /// 需要审批
    ApprovalRequired {
        /// 会话 id
        session_id: i64,
        /// 调用 id
        call_id: String,
        /// 工具名
        tool: String,
        /// 主参数
        arg: String,
        /// 原因
        reason: String,
    },
    /// 审批结束（含 auto 批准）
    ApprovalResolved {
        /// 会话 id
        session_id: i64,
        /// 调用 id
        call_id: String,
        /// 决断
        decision: String,
    },
    /// TODO 推进
    TodoUpdate {
        /// 会话 id
        session_id: i64,
        /// TODO 列表
        todos: Vec<TodoDTO>,
    },
    /// 产生文件变更
    ChangeAdd {
        /// 会话 id
        session_id: i64,
        /// 变更
        change: ChangeDTO,
    },
    /// 上下文/token 统计
    CtxUpdate {
        /// 会话 id
        session_id: i64,
        /// 上下文占用百分比
        ctx_percent: i64,
        /// token 统计
        tokens: TokenUsageDTO,
    },
    /// 自动压缩完成
    Compacted {
        /// 会话 id
        session_id: i64,
        /// 摘要
        summary: String,
    },
    /// 自动续跑（前端展示「自动继续执行」提示）
    RunContinued {
        /// 会话 id
        session_id: i64,
        /// 第几次续跑（从 1 开始）
        round: i64,
    },
    /// 工具增量输出（Bash 终端式滚动）
    ToolDelta {
        /// 会话 id
        session_id: i64,
        /// 调用 id
        call_id: String,
        /// 输出增量
        delta: String,
    },
    /// 运行结束
    RunEnd {
        /// 会话 id
        session_id: i64,
        /// 结果（done/aborted/error）
        result: String,
        /// 错误信息（result=error 时存在）
        message: Option<String>,
        /// 本次运行 token 用量
        usage: TokenUsageDTO,
    },
}

impl From<AgentEvent> for AgentEventDTO {
    fn from(e: AgentEvent) -> Self {
        match e {
            AgentEvent::TextDelta { session_id, delta } => Self::TextDelta { session_id, delta },
            AgentEvent::ThinkingDelta { session_id, delta } => {
                Self::ThinkingDelta { session_id, delta }
            }
            AgentEvent::ToolStart {
                session_id,
                call_id,
                tool,
                arg,
            } => Self::ToolStart {
                session_id,
                call_id,
                tool,
                arg,
            },
            AgentEvent::ToolEnd {
                session_id,
                call_id,
                status,
                output,
                note,
            } => Self::ToolEnd {
                session_id,
                call_id,
                status,
                output,
                note,
            },
            AgentEvent::ApprovalRequired {
                session_id,
                call_id,
                tool,
                arg,
                reason,
            } => Self::ApprovalRequired {
                session_id,
                call_id,
                tool,
                arg,
                reason,
            },
            AgentEvent::ApprovalResolved {
                session_id,
                call_id,
                decision,
            } => Self::ApprovalResolved {
                session_id,
                call_id,
                decision,
            },
            AgentEvent::TodoUpdate { session_id, todos } => Self::TodoUpdate {
                session_id,
                todos: todos
                    .into_iter()
                    .map(|t| TodoDTO {
                        id: t.id,
                        content: t.content,
                        status: t.status,
                    })
                    .collect(),
            },
            AgentEvent::ChangeAdd { session_id, change } => Self::ChangeAdd {
                session_id,
                change: ChangeDTO {
                    change_id: change.change_id,
                    file_path: change.file_path,
                    add_lines: change.add_lines,
                    del_lines: change.del_lines,
                    rolled_back: change.rolled_back,
                },
            },
            AgentEvent::CtxUpdate {
                session_id,
                ctx_percent,
                tokens,
            } => Self::CtxUpdate {
                session_id,
                ctx_percent,
                tokens: TokenUsageDTO {
                    input: tokens.input,
                    output: tokens.output,
                },
            },
            AgentEvent::Compacted { session_id, summary } => Self::Compacted { session_id, summary },
            AgentEvent::RunContinued { session_id, round } => Self::RunContinued { session_id, round },
            AgentEvent::ToolDelta {
                session_id,
                call_id,
                delta,
            } => Self::ToolDelta {
                session_id,
                call_id,
                delta,
            },
            AgentEvent::RunEnd {
                session_id,
                result,
                usage,
            } => {
                let (result, message) = match result {
                    RunResult::Done => ("done".to_string(), None),
                    RunResult::Aborted => ("aborted".to_string(), None),
                    RunResult::Error(m) => ("error".to_string(), Some(m)),
                };
                Self::RunEnd {
                    session_id,
                    result,
                    message,
                    usage: TokenUsageDTO {
                        input: usage.input,
                        output: usage.output,
                    },
                }
            }
        }
    }
}
