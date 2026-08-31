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

/// 用户消息内嵌图片（mime + base64 data，不含 data: 前缀）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageImage {
    /// MIME 类型（image/png、image/jpeg、image/webp、image/gif）
    pub mime: String,
    /// base64 编码的图片数据
    pub data: String,
}

impl MessageImage {
    /// 允许的图片 MIME 白名单
    pub const ALLOWED_MIMES: [&'static str; 4] =
        ["image/png", "image/jpeg", "image/webp", "image/gif"];
    /// 单图 base64 长度上限（约 6MB 原始字节）
    pub const MAX_B64_LEN: usize = 8_000_000;

    /// 校验并构造：mime 归一化（image/jpg → image/jpeg）且必须在白名单，
    /// data 必须为合法 base64 字符集且长度合法；非法返回 None
    pub fn parse(mime: &str, data: &str) -> Option<Self> {
        let mime = if mime.eq_ignore_ascii_case("image/jpg") {
            "image/jpeg".to_string()
        } else {
            mime.trim().to_ascii_lowercase()
        };
        if !Self::ALLOWED_MIMES.contains(&mime.as_str()) {
            return None;
        }
        if data.is_empty() || data.len() > Self::MAX_B64_LEN {
            return None;
        }
        // base64 字符集与填充校验（不整体解码，避免大串内存复制）
        let padding = data.len() - data.trim_end_matches('=').len();
        if data.len() % 4 != 0 || padding > 2 {
            return None;
        }
        if !data
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/' || b == b'=')
        {
            return None;
        }
        Some(Self { mime, data: data.to_string() })
    }
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

    /// 构造用户消息载荷：无图片时与 text_payload 同形；有图片时附加 `images` 数组
    pub fn user_payload(text: &str, images: &[MessageImage]) -> String {
        if images.is_empty() {
            return Self::text_payload(text);
        }
        serde_json::json!({
            "text": text,
            "images": images
                .iter()
                .map(|i| serde_json::json!({ "mime": i.mime, "data": i.data }))
                .collect::<Vec<_>>(),
        })
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_image_parse_validates() {
        // 合法：标准 base64 + 4 倍数长度
        let img = MessageImage::parse("image/png", "aGVsbG8=").unwrap();
        assert_eq!(img.mime, "image/png");
        // image/jpg 归一化为 image/jpeg；mime 大小写不敏感
        let img = MessageImage::parse("Image/JPG", "aGVsbG8=").unwrap();
        assert_eq!(img.mime, "image/jpeg");
        // 非法 MIME
        assert!(MessageImage::parse("image/svg+xml", "aGVsbG8=").is_none());
        assert!(MessageImage::parse("text/plain", "aGVsbG8=").is_none());
        // 非法 base64：长度非 4 倍数 / 填充过多 / 非法字符
        assert!(MessageImage::parse("image/png", "abc").is_none());
        assert!(MessageImage::parse("image/png", "a====").is_none());
        assert!(MessageImage::parse("image/png", "aGVs**8=").is_none());
        assert!(MessageImage::parse("image/png", "").is_none());
        // 超长拒绝
        let big = "A".repeat(MessageImage::MAX_B64_LEN + 1);
        assert!(MessageImage::parse("image/png", &big).is_none());
    }

    #[test]
    fn user_payload_shapes() {
        // 无图：无 images 键
        let v: serde_json::Value =
            serde_json::from_str(&Message::user_payload("hi", &[])).unwrap();
        assert_eq!(v["text"], "hi");
        assert!(v.get("images").is_none());
        // 有图：images 数组按序携带
        let imgs = vec![
            MessageImage { mime: "image/png".into(), data: "aGk=".into() },
            MessageImage { mime: "image/webp".into(), data: "eW8=".into() },
        ];
        let v: serde_json::Value =
            serde_json::from_str(&Message::user_payload("看图", &imgs)).unwrap();
        assert_eq!(v["images"].as_array().unwrap().len(), 2);
        assert_eq!(v["images"][0]["mime"], "image/png");
        assert_eq!(v["images"][1]["data"], "eW8=");
    }
}
