//! Agent 事件（domain 侧纯数据定义，adapter 转 DTO 后经 `agent:event` 单通道推送）。

/// token 用量统计
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TokenUsage {
    /// 输入 token
    pub input: i64,
    /// 输出 token
    pub output: i64,
}

/// TODO 项
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TodoItem {
    /// 序号
    pub id: i64,
    /// 内容
    pub content: String,
    /// 状态（pending/in_progress/done）
    pub status: String,
}

/// 文件变更信息（checkpoint）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeInfo {
    /// 变更 id（checkpoint 主键）
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

/// 运行结束类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunResult {
    /// 正常完成
    Done,
    /// 被中断
    Aborted,
    /// 出错（携带错误信息）
    Error(String),
}

impl RunResult {
    /// 事件字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Done => "done",
            Self::Aborted => "aborted",
            Self::Error(_) => "error",
        }
    }
}

/// Agent 事件（`agent:event` 单通道 + type 判别）
#[derive(Debug, Clone, PartialEq)]
pub enum AgentEvent {
    /// LLM 流式文本
    TextDelta {
        /// 会话 id
        session_id: i64,
        /// 文本增量
        delta: String,
    },
    /// LLM 流式思考过程（推理模型 reasoning_content）
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
        /// 状态（ok/error）
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
        /// 决断（once/always/reject/auto/abort）
        decision: String,
    },
    /// TODO 推进
    TodoUpdate {
        /// 会话 id
        session_id: i64,
        /// TODO 列表
        todos: Vec<TodoItem>,
    },
    /// 产生文件变更（checkpoint）
    ChangeAdd {
        /// 会话 id
        session_id: i64,
        /// 变更信息
        change: ChangeInfo,
    },
    /// 上下文/token 统计
    CtxUpdate {
        /// 会话 id
        session_id: i64,
        /// 上下文占用百分比
        ctx_percent: i64,
        /// token 统计
        tokens: TokenUsage,
    },
    /// 自动压缩完成
    Compacted {
        /// 会话 id
        session_id: i64,
        /// 摘要
        summary: String,
    },
    /// 运行结束
    RunEnd {
        /// 会话 id
        session_id: i64,
        /// 结果（done/aborted/error）
        result: RunResult,
        /// 本次运行 token 用量
        usage: TokenUsage,
    },
}

/// 运行事件推送端口（adapter/event.rs 用 AppHandle 实现，保持 application 不依赖 tauri）
pub trait RunEventSink: Send + Sync {
    /// 推送事件到前端
    fn emit(&self, event: AgentEvent);
}
