//! 领域错误：领域行为（校验、状态流转、权限判定）的失败类型。

/// 领域错误
#[derive(Debug, thiserror::Error)]
pub enum DomainError {
    /// 参数/业务规则校验失败
    #[error("校验失败：{0}")]
    Validation(String),
    /// 资源不存在
    #[error("资源不存在：{0}")]
    NotFound(String),
    /// 业务冲突（重名、运行冲突、默认保护）
    #[error("业务冲突：{0}")]
    Conflict(String),
    /// 非法状态流转
    #[error("非法状态：{0}")]
    State(String),
    /// 权限引擎 deny 命中
    #[error("操作被拒绝：{0}")]
    Denied(String),
}
