//! 回收站命令对象。

use crate::domain::DomainError;

/// 回收站对象类别
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecycleKind {
    /// 会话（连带消息；项目软删时向上级联恢复项目）
    Session,
    /// 项目（级联恢复同窗删除的会话/消息/checkpoint/项目级规则）
    Project,
    /// 模型配置
    Model,
    /// MCP 服务器
    Mcp,
    /// 插件（恢复后保持 disabled 待手动启用）
    Plugin,
    /// 权限规则
    PermRule,
}

impl RecycleKind {
    /// 从请求字符串解析
    pub fn parse(s: &str) -> Result<Self, DomainError> {
        match s {
            "session" => Ok(Self::Session),
            "project" => Ok(Self::Project),
            "model" => Ok(Self::Model),
            "mcp" => Ok(Self::Mcp),
            "plugin" => Ok(Self::Plugin),
            "permRule" => Ok(Self::PermRule),
            other => Err(DomainError::Validation(format!("非法回收对象类别：{other}"))),
        }
    }
}

/// 恢复回收站对象命令
#[derive(Debug, Clone)]
pub struct RestoreRecycleItemCmd {
    /// 对象类别（session/project/model/mcp/plugin/permRule）
    pub kind: String,
    /// 对象 id
    pub id: i64,
}
