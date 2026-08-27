//! Session：会话充血对象（消息序号自洽、compaction 判定与执行）。

use chrono::NaiveDateTime;

use super::{Message, MessageKind};

/// 上下文压缩触发阈值（百分比）
pub const COMPACT_THRESHOLD: i64 = 90;

/// 会话
#[derive(Debug, Clone)]
pub struct Session {
    /// 主键 id（插入后回填）
    pub id: i64,
    /// 所属项目 id
    pub project_id: i64,
    /// 会话标题（首条任务截断生成）
    pub title: String,
    /// 上下文占用百分比（0-100）
    pub ctx_percent: i64,
    /// 累计输入 token
    pub input_tokens: i64,
    /// 累计输出 token
    pub output_tokens: i64,
    /// 会话内消息（按 seq 升序）
    pub messages: Vec<Message>,
    /// 创建时间
    pub created_at: NaiveDateTime,
    /// 更新时间
    pub updated_at: NaiveDateTime,
}

impl Session {
    /// 新建会话（未持久化，id 待回填）
    pub fn new(project_id: i64, now: NaiveDateTime) -> Self {
        Self {
            id: 0,
            project_id,
            title: "新会话".to_string(),
            ctx_percent: 0,
            input_tokens: 0,
            output_tokens: 0,
            messages: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    /// 首条任务生成标题（截断 20 字符）
    pub fn apply_first_task_title(&mut self, text: &str) {
        if self.title != "新会话" {
            return;
        }
        let trimmed = text.trim().replace('\n', " ");
        let title: String = trimmed.chars().take(20).collect();
        if !title.is_empty() {
            self.title = title;
        }
    }

    /// 追加消息：序号自洽（last seq + 1）
    pub fn append_message(&mut self, mut message: Message) -> &Message {
        message.seq = self.messages.last().map(|m| m.seq + 1).unwrap_or(1);
        self.messages.push(message);
        self.messages.last().expect("刚追加的消息必然存在")
    }

    /// 是否需要压缩：ctx 占用 ≥ threshold（默认 90%）
    pub fn should_compact(&self, threshold: i64) -> bool {
        self.ctx_percent >= threshold
    }

    /// 执行压缩：保留全部用户消息与最近 40% 消息，其余替换为一条 system 摘要消息。
    /// 返回被移除的消息数。
    pub fn compact(&mut self, summary: String, now: NaiveDateTime) -> usize {
        let total = self.messages.len();
        if total == 0 {
            return 0;
        }
        let keep_tail = total * 40 / 100;
        let tail_start = total.saturating_sub(keep_tail);
        let mut kept: Vec<Message> = Vec::new();
        let mut removed = 0usize;
        for (idx, msg) in self.messages.iter().enumerate() {
            if msg.kind == MessageKind::User || idx >= tail_start {
                kept.push(msg.clone());
            } else {
                removed += 1;
            }
        }
        let mut summary_msg = Message::new(self.id, MessageKind::System, summary, now);
        // 摘要消息排在最前，序号重排由调用方（重写落库）时保持一致
        summary_msg.seq = 0;
        let mut next = std::iter::once(summary_msg).chain(kept).enumerate();
        self.messages = Vec::new();
        for (i, mut m) in &mut next {
            m.seq = (i + 1) as i64;
            self.messages.push(m);
        }
        removed
    }

    /// 更新 token 统计与上下文占用
    pub fn update_usage(&mut self, input: i64, output: i64, ctx_percent: i64) {
        self.input_tokens += input;
        self.output_tokens += output;
        self.ctx_percent = ctx_percent.clamp(0, 100);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(session_id: i64, kind: MessageKind, text: &str) -> Message {
        Message::new(
            session_id,
            kind,
            Message::text_payload(text),
            NaiveDateTime::default(),
        )
    }

    #[test]
    fn append_message_seq_increments() {
        let mut s = Session::new(1, NaiveDateTime::default());
        s.id = 7;
        s.append_message(msg(7, MessageKind::User, "u1"));
        s.append_message(msg(7, MessageKind::Assistant, "a1"));
        assert_eq!(s.messages[0].seq, 1);
        assert_eq!(s.messages[1].seq, 2);
    }

    #[test]
    fn should_compact_at_threshold() {
        let mut s = Session::new(1, NaiveDateTime::default());
        s.ctx_percent = 89;
        assert!(!s.should_compact(COMPACT_THRESHOLD));
        s.ctx_percent = 90;
        assert!(s.should_compact(COMPACT_THRESHOLD));
        s.ctx_percent = 95;
        assert!(s.should_compact(COMPACT_THRESHOLD));
    }

    #[test]
    fn compact_keeps_user_and_recent_tail() {
        let mut s = Session::new(1, NaiveDateTime::default());
        s.id = 1;
        // 10 条消息：seq1 user、seq2-6 assistant/tool（待压缩区）、seq7-10 最近 40%
        s.append_message(msg(1, MessageKind::User, "需求"));
        for i in 0..5 {
            s.append_message(msg(1, MessageKind::Assistant, &format!("步骤{i}")));
        }
        for i in 0..4 {
            s.append_message(msg(1, MessageKind::Assistant, &format!("尾部{i}")));
        }
        let removed = s.compact(Message::text_payload("摘要"), NaiveDateTime::default());
        // 保留：1 条 user + 4 条尾部 + 1 条摘要 = 6 条；移除 5 条
        assert_eq!(removed, 5);
        assert_eq!(s.messages.len(), 6);
        assert_eq!(s.messages[0].kind, MessageKind::System);
        assert!(s.messages.iter().any(|m| m.kind == MessageKind::User));
        // 序号连续
        for (i, m) in s.messages.iter().enumerate() {
            assert_eq!(m.seq, (i + 1) as i64);
        }
    }

    #[test]
    fn first_task_title_truncates() {
        let mut s = Session::new(1, NaiveDateTime::default());
        s.apply_first_task_title("帮我修复 approval 的中断 bug，这个问题很紧急需要尽快处理");
        assert_eq!(s.title.chars().count(), 20);
        s.apply_first_task_title("第二条任务不应覆盖标题");
        assert!(s.title.starts_with("帮我修复"));
    }
}
