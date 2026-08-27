//! 会话业务对象（Domain → BO，adapter 再转 DTO）。

use chrono::NaiveDateTime;

use crate::domain::session::{Message, Session};

/// 会话摘要 BO（列表项）
#[derive(Debug, Clone)]
pub struct SessionSummaryBO {
    /// 会话 id
    pub id: i64,
    /// 标题
    pub title: String,
    /// 上下文占用百分比
    pub ctx_percent: i64,
    /// 累计输入 token
    pub input_tokens: i64,
    /// 累计输出 token
    pub output_tokens: i64,
    /// 创建时间
    pub created_at: NaiveDateTime,
    /// 更新时间
    pub updated_at: NaiveDateTime,
}

impl From<Session> for SessionSummaryBO {
    fn from(s: Session) -> Self {
        Self {
            id: s.id,
            title: s.title,
            ctx_percent: s.ctx_percent,
            input_tokens: s.input_tokens,
            output_tokens: s.output_tokens,
            created_at: s.created_at,
            updated_at: s.updated_at,
        }
    }
}

/// 消息 BO
#[derive(Debug, Clone)]
pub struct MessageBO {
    /// 消息 id
    pub id: i64,
    /// 会话内序号
    pub seq: i64,
    /// 消息类型
    pub kind: String,
    /// JSON 载荷
    pub payload: String,
    /// 创建时间
    pub created_at: NaiveDateTime,
}

impl From<Message> for MessageBO {
    fn from(m: Message) -> Self {
        Self {
            id: m.id,
            seq: m.seq,
            kind: m.kind.as_str().to_string(),
            payload: m.payload,
            created_at: m.created_at,
        }
    }
}

/// 会话详情 BO（含全部消息）
#[derive(Debug, Clone)]
pub struct SessionBO {
    /// 会话 id
    pub id: i64,
    /// 所属项目 id
    pub project_id: i64,
    /// 标题
    pub title: String,
    /// 上下文占用百分比
    pub ctx_percent: i64,
    /// 累计输入 token
    pub input_tokens: i64,
    /// 累计输出 token
    pub output_tokens: i64,
    /// 消息列表（seq 升序）
    pub messages: Vec<MessageBO>,
    /// 创建时间
    pub created_at: NaiveDateTime,
    /// 更新时间
    pub updated_at: NaiveDateTime,
}

impl From<Session> for SessionBO {
    fn from(s: Session) -> Self {
        Self {
            id: s.id,
            project_id: s.project_id,
            title: s.title,
            ctx_percent: s.ctx_percent,
            input_tokens: s.input_tokens,
            output_tokens: s.output_tokens,
            messages: s.messages.into_iter().map(MessageBO::from).collect(),
            created_at: s.created_at,
            updated_at: s.updated_at,
        }
    }
}
