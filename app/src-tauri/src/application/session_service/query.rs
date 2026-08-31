//! 会话查询对象。

/// 会话列表查询
#[derive(Debug, Clone)]
pub struct ListSessionQuery {
    /// 项目路径
    pub project_path: String,
    /// 标题关键字（可选）
    pub keyword: Option<String>,
}

/// 打开会话查询
#[derive(Debug, Clone)]
pub struct GetSessionQuery {
    /// 会话 id
    pub session_id: i64,
}

/// 消息分页查询（聊天窗口：游标向前加载历史）
#[derive(Debug, Clone)]
pub struct ListMessagesQuery {
    /// 会话 id
    pub session_id: i64,
    /// 游标：取 seq < before_seq 的消息；None = 从尾部开始
    pub before_seq: Option<i64>,
    /// 本页条数上限（1..=200，超界收敛）
    pub limit: i64,
}

/// 项目 token 用量查询
#[derive(Debug, Clone)]
pub struct ProjectTokenUsageQuery {
    /// 项目路径
    pub project_path: String,
}
