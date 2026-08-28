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

/// 项目 token 用量查询
#[derive(Debug, Clone)]
pub struct ProjectTokenUsageQuery {
    /// 项目路径
    pub project_path: String,
}
