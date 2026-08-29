//! 会话命令对象（adapter Request → Cmd 后传入）。

/// 创建会话命令
#[derive(Debug, Clone)]
pub struct CreateSessionCmd {
    /// 项目路径（须已 open_project 注册）
    pub project_path: String,
}

/// 删除会话命令（软删）
#[derive(Debug, Clone)]
pub struct DeleteSessionCmd {
    /// 会话 id
    pub session_id: i64,
}

/// 恢复会话命令（回收站）
#[derive(Debug, Clone)]
pub struct RestoreSessionCmd {
    /// 会话 id
    pub id: i64,
}

/// 编辑消息命令（编辑即截断：更新文本 + 物理删除后续消息）
#[derive(Debug, Clone)]
pub struct EditMessageCmd {
    /// 消息 id
    pub id: i64,
    /// 新文本
    pub text: String,
}

/// 设置会话级模型偏好命令
#[derive(Debug, Clone)]
pub struct SetSessionModelCmd {
    /// 会话 id
    pub session_id: i64,
    /// 模型名（trim 后为空串 = 清除偏好，跟随全局）
    pub model: String,
}

/// 重命名会话命令
#[derive(Debug, Clone)]
pub struct RenameSessionCmd {
    /// 会话 id
    pub id: i64,
    /// 新标题（trim 后 1..=80 字符）
    pub title: String,
}

/// 追加消息命令（AgentService 内部复用）
#[derive(Debug, Clone)]
pub struct AppendMessageCmd {
    /// 会话 id
    pub session_id: i64,
    /// 消息类型（user/assistant/tool/approval/system）
    pub kind: String,
    /// JSON 载荷
    pub payload: String,
}
