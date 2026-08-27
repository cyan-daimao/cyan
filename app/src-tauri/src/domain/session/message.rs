//! Message：会话内消息（user/assistant/tool/approval/system）。

use chrono::NaiveDateTime;

use crate::domain::DomainError;

/// 消息类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
    /// 用户消息
    User,
    /// 助手消息（文本 / 工具调用）
    Assistant,
    /// 工具结果消息
    Tool,
    /// 审批消息
    Approval,
    /// 系统消息（compaction 摘要等）
    System,
}

impl MessageKind {
    /// 存储字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::Tool => "tool",
            Self::Approval => "approval",
            Self::System => "system",
        }
    }

    /// 从存储字符串解析
    pub fn parse(s: &str) -> Result<Self, DomainError> {
        match s {
            "user" => Ok(Self::User),
            "assistant" => Ok(Self::Assistant),
            "tool" => Ok(Self::Tool),
            "approval" => Ok(Self::Approval),
            "system" => Ok(Self::System),
            other => Err(DomainError::Validation(format!("未知消息类型：{other}"))),
        }
    }
}

/// 会话消息
#[derive(Debug, Clone)]
pub struct Message {
    /// 主键 id（插入后回填）
    pub id: i64,
    /// 所属会话 id
    pub session_id: i64,
    /// 会话内序号（自增，append_message 保证自洽）
    pub seq: i64,
    /// 消息类型
    pub kind: MessageKind,
    /// JSON 载荷（文本或工具卡/审批卡结构）
    pub payload: String,
    /// 创建时间
    pub created_at: NaiveDateTime,
    /// 更新时间
    pub updated_at: NaiveDateTime,
}

impl Message {
    /// 新建（seq 由 Session::append_message 分配）
    pub fn new(session_id: i64, kind: MessageKind, payload: String, now: NaiveDateTime) -> Self {
        Self {
            id: 0,
            session_id,
            seq: 0,
            kind,
            payload,
            created_at: now,
            updated_at: now,
        }
    }

    /// 提取载荷中的纯文本（`{"text": ...}`），非文本载荷返回 None
    pub fn text(&self) -> Option<String> {
        serde_json::from_str::<serde_json::Value>(&self.payload)
            .ok()
            .and_then(|v| v.get("text").and_then(|t| t.as_str().map(String::from)))
    }

    /// 构造文本载荷
    pub fn text_payload(text: &str) -> String {
        serde_json::json!({ "text": text }).to_string()
    }
}
