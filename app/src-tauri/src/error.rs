//! 统一服务错误：所有 Tauri command 以 `Err(ServiceError)` 返回，序列化为 `{ code, message }`。
//! 错误码约定（TECH_DESIGN 第 7 章）：1xxx 参数/校验错；2xxx 业务错；3xxx 外部依赖错；9001 未授权。

use serde::Serialize;

use crate::domain::DomainError;

/// 统一服务错误，Tauri command 的错误返回类型
#[derive(Debug, Clone, Serialize)]
pub struct ServiceError {
    /// 错误码
    pub code: i32,
    /// 错误信息
    pub message: String,
}

impl ServiceError {
    /// 构造任意错误
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    /// 1001 参数/校验错
    pub fn validation(message: impl Into<String>) -> Self {
        Self::new(1001, message)
    }

    /// 2002 资源不存在
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(2002, message)
    }

    /// 2001 业务冲突（运行冲突、重名、默认保护等）
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(2001, message)
    }

    /// 2003 非法状态流转
    pub fn state(message: impl Into<String>) -> Self {
        Self::new(2003, message)
    }

    /// 9001 未授权操作（deny 命中）
    pub fn denied(message: impl Into<String>) -> Self {
        Self::new(9001, message)
    }

    /// 3000 外部依赖错（LLM/MCP/git/DB）
    pub fn external(message: impl Into<String>) -> Self {
        Self::new(3000, message)
    }
}

impl std::fmt::Display for ServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for ServiceError {}

impl From<DomainError> for ServiceError {
    fn from(e: DomainError) -> Self {
        match e {
            DomainError::Validation(m) => Self::validation(m),
            DomainError::NotFound(m) => Self::not_found(m),
            DomainError::Conflict(m) => Self::conflict(m),
            DomainError::State(m) => Self::state(m),
            DomainError::Denied(m) => Self::denied(m),
        }
    }
}

impl From<anyhow::Error> for ServiceError {
    fn from(e: anyhow::Error) -> Self {
        Self::external(format!("{e:#}"))
    }
}
