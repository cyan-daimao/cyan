//! Agent 命令对象。

/// 发起 Agent 任务命令
#[derive(Debug, Clone)]
pub struct StartRunCmd {
    /// 会话 id
    pub session_id: i64,
    /// 任务文本
    pub text: String,
    /// 模型名（空串表示使用默认模型）
    pub model: String,
    /// 权限模式（ask/auto/plan）
    pub perm_mode: String,
}

/// 中断当前运行命令
#[derive(Debug, Clone)]
pub struct InterruptCmd {
    /// 会话 id
    pub session_id: i64,
}

/// 审批命令
#[derive(Debug, Clone)]
pub struct ApproveCmd {
    /// 会话 id
    pub session_id: i64,
    /// 调用 id
    pub call_id: String,
    /// 决断（once/always/reject）
    pub decision: String,
}

/// 回滚变更命令
#[derive(Debug, Clone)]
pub struct RollbackCmd {
    /// 会话 id
    pub session_id: i64,
    /// 变更 id（checkpoint 主键）
    pub change_id: i64,
}
