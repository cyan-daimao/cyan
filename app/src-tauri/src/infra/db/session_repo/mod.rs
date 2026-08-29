//! 会话仓储：SessionDO/MessageDO + Impl + From 转换（自增 id 回填、软删过滤）。

use async_trait::async_trait;
use sqlx::{FromRow, SqlitePool};

use crate::domain::session::{Message, MessageKind, MessageRepository, Session, SessionRepository};

use super::{fmt_time, now_local, parse_time};

/// 会话表行（cyan_session）
#[derive(Debug, FromRow)]
pub struct SessionDO {
    /// 主键 id
    pub id: i64,
    /// 所属项目 id
    pub project_id: i64,
    /// 会话标题
    pub title: String,
    /// 上下文占用百分比
    pub ctx_percent: i64,
    /// 累计输入 token
    pub input_tokens: i64,
    /// 累计输出 token
    pub output_tokens: i64,
    /// 创建人
    pub created_by: String,
    /// 更新人
    pub updated_by: String,
    /// 创建时间（YYYY-MM-DD HH:MM:SS）
    pub created_at: String,
    /// 更新时间
    pub updated_at: String,
    /// 删除时间（软删）
    pub deleted_at: Option<String>,
}

impl TryFrom<SessionDO> for Session {
    type Error = anyhow::Error;

    fn try_from(d: SessionDO) -> anyhow::Result<Self> {
        Ok(Self {
            id: d.id,
            project_id: d.project_id,
            title: d.title,
            ctx_percent: d.ctx_percent,
            input_tokens: d.input_tokens,
            output_tokens: d.output_tokens,
            messages: Vec::new(),
            created_at: parse_time(&d.created_at)?,
            updated_at: parse_time(&d.updated_at)?,
            deleted_at: d.deleted_at.as_deref().map(parse_time).transpose()?,
        })
    }
}

/// 消息表行（cyan_message）
#[derive(Debug, FromRow)]
pub struct MessageDO {
    /// 主键 id
    pub id: i64,
    /// 所属会话 id
    pub session_id: i64,
    /// 会话内序号
    pub seq: i64,
    /// 消息类型（user/assistant/tool/approval/system）
    pub kind: String,
    /// JSON 载荷
    pub payload: String,
    /// 创建人
    pub created_by: String,
    /// 更新人
    pub updated_by: String,
    /// 创建时间
    pub created_at: String,
    /// 更新时间
    pub updated_at: String,
    /// 删除时间（软删）
    pub deleted_at: Option<String>,
}

impl TryFrom<MessageDO> for Message {
    type Error = anyhow::Error;

    fn try_from(d: MessageDO) -> anyhow::Result<Self> {
        Ok(Self {
            id: d.id,
            session_id: d.session_id,
            seq: d.seq,
            kind: MessageKind::parse(&d.kind)?,
            payload: d.payload,
            created_at: parse_time(&d.created_at)?,
            updated_at: parse_time(&d.updated_at)?,
        })
    }
}

/// 会话仓储 SQLx 实现
pub struct SessionRepositoryImpl {
    pool: SqlitePool,
}

impl SessionRepositoryImpl {
    /// 构造
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SessionRepository for SessionRepositoryImpl {
    async fn find_by_id(&self, id: i64) -> anyhow::Result<Option<Session>> {
        let row = sqlx::query_as::<_, SessionDO>(
            "SELECT id, project_id, title, ctx_percent, input_tokens, output_tokens,
                    created_by, updated_by, created_at, updated_at, deleted_at
             FROM cyan_session WHERE id = ? AND deleted_at IS NULL",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(Session::try_from).transpose()
    }

    async fn list_by_project(
        &self,
        project_id: i64,
        keyword: Option<&str>,
    ) -> anyhow::Result<Vec<Session>> {
        let rows = match keyword {
            Some(kw) if !kw.trim().is_empty() => {
                sqlx::query_as::<_, SessionDO>(
                    "SELECT id, project_id, title, ctx_percent, input_tokens, output_tokens,
                            created_by, updated_by, created_at, updated_at, deleted_at
                     FROM cyan_session
                     WHERE project_id = ? AND deleted_at IS NULL AND title LIKE ?
                     ORDER BY updated_at DESC",
                )
                .bind(project_id)
                .bind(format!("%{}%", kw.trim()))
                .fetch_all(&self.pool)
                .await?
            }
            _ => {
                sqlx::query_as::<_, SessionDO>(
                    "SELECT id, project_id, title, ctx_percent, input_tokens, output_tokens,
                            created_by, updated_by, created_at, updated_at, deleted_at
                     FROM cyan_session
                     WHERE project_id = ? AND deleted_at IS NULL
                     ORDER BY updated_at DESC",
                )
                .bind(project_id)
                .fetch_all(&self.pool)
                .await?
            }
        };
        rows.into_iter().map(Session::try_from).collect()
    }

    async fn insert(&self, session: &mut Session) -> anyhow::Result<()> {
        let now = now_local();
        session.created_at = now;
        session.updated_at = now;
        let id = sqlx::query(
            "INSERT INTO cyan_session
                (project_id, title, ctx_percent, input_tokens, output_tokens,
                 created_by, updated_by, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, 'local', 'local', ?, ?)",
        )
        .bind(session.project_id)
        .bind(&session.title)
        .bind(session.ctx_percent)
        .bind(session.input_tokens)
        .bind(session.output_tokens)
        .bind(fmt_time(&now))
        .bind(fmt_time(&now))
        .execute(&self.pool)
        .await?
        .last_insert_rowid();
        session.id = id;
        Ok(())
    }

    async fn update(&self, session: &Session) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE cyan_session
             SET title = ?, ctx_percent = ?, input_tokens = ?, output_tokens = ?,
                 updated_by = 'local', updated_at = ?
             WHERE id = ? AND deleted_at IS NULL",
        )
        .bind(&session.title)
        .bind(session.ctx_percent)
        .bind(session.input_tokens)
        .bind(session.output_tokens)
        .bind(fmt_time(&now_local()))
        .bind(session.id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn soft_delete(&self, id: i64) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE cyan_session SET deleted_at = ?, updated_by = 'local', updated_at = ?
             WHERE id = ? AND deleted_at IS NULL",
        )
        .bind(fmt_time(&now_local()))
        .bind(fmt_time(&now_local()))
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn sum_tokens_by_project(&self, project_id: i64) -> anyhow::Result<(i64, i64, i64)> {
        let row: (i64, i64, i64) = sqlx::query_as(
            "SELECT COALESCE(SUM(input_tokens), 0), COALESCE(SUM(output_tokens), 0), COUNT(*)
             FROM cyan_session WHERE project_id = ? AND deleted_at IS NULL",
        )
        .bind(project_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    async fn list_deleted(&self) -> anyhow::Result<Vec<Session>> {
        let rows = sqlx::query_as::<_, SessionDO>(
            "SELECT id, project_id, title, ctx_percent, input_tokens, output_tokens,
                    created_by, updated_by, created_at, updated_at, deleted_at
             FROM cyan_session WHERE deleted_at IS NOT NULL
             ORDER BY deleted_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(Session::try_from).collect()
    }

    async fn restore(&self, id: i64) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE cyan_session SET deleted_at = NULL, updated_by = 'local', updated_at = ?
             WHERE id = ? AND deleted_at IS NOT NULL",
        )
        .bind(fmt_time(&now_local()))
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

/// 消息仓储 SQLx 实现
pub struct MessageRepositoryImpl {
    pool: SqlitePool,
}

impl MessageRepositoryImpl {
    /// 构造
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl MessageRepository for MessageRepositoryImpl {
    async fn list_by_session(&self, session_id: i64) -> anyhow::Result<Vec<Message>> {
        let rows = sqlx::query_as::<_, MessageDO>(
            "SELECT id, session_id, seq, kind, payload,
                    created_by, updated_by, created_at, updated_at, deleted_at
             FROM cyan_message
             WHERE session_id = ? AND deleted_at IS NULL
             ORDER BY seq ASC",
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(Message::try_from).collect()
    }

    async fn insert(&self, message: &mut Message) -> anyhow::Result<()> {
        let now = now_local();
        message.created_at = now;
        message.updated_at = now;
        let id = sqlx::query(
            "INSERT INTO cyan_message
                (session_id, seq, kind, payload, created_by, updated_by, created_at, updated_at)
             VALUES (?, ?, ?, ?, 'local', 'local', ?, ?)",
        )
        .bind(message.session_id)
        .bind(message.seq)
        .bind(message.kind.as_str())
        .bind(&message.payload)
        .bind(fmt_time(&now))
        .bind(fmt_time(&now))
        .execute(&self.pool)
        .await?
        .last_insert_rowid();
        message.id = id;
        Ok(())
    }

    async fn update_payload(&self, id: i64, payload: &str) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE cyan_message SET payload = ?, updated_by = 'local', updated_at = ?
             WHERE id = ? AND deleted_at IS NULL",
        )
        .bind(payload)
        .bind(fmt_time(&now_local()))
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn soft_delete_by_session(&self, session_id: i64) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE cyan_message SET deleted_at = ?, updated_by = 'local', updated_at = ?
             WHERE session_id = ? AND deleted_at IS NULL",
        )
        .bind(fmt_time(&now_local()))
        .bind(fmt_time(&now_local()))
        .bind(session_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn restore_by_session(&self, session_id: i64) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE cyan_message SET deleted_at = NULL, updated_by = 'local', updated_at = ?
             WHERE session_id = ? AND deleted_at IS NOT NULL",
        )
        .bind(fmt_time(&now_local()))
        .bind(session_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::session::MessageKind;

    async fn seed_project(pool: &SqlitePool) -> i64 {
        sqlx::query(
            "INSERT INTO cyan_project (name, path, created_by, updated_by, created_at, updated_at)
             VALUES ('demo', '/tmp/demo', 'local', 'local', '2026-08-27 10:00:00', '2026-08-27 10:00:00')",
        )
        .execute(pool)
        .await
        .unwrap()
        .last_insert_rowid()
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn session_insert_backfills_id_and_soft_delete_filters(pool: SqlitePool) {
        let project_id = seed_project(&pool).await;
        let repo = SessionRepositoryImpl::new(pool.clone());

        let mut s = Session::new(project_id, now_local());
        repo.insert(&mut s).await.unwrap();
        assert!(s.id > 0, "插入后应回填自增 id");

        let found = repo.find_by_id(s.id).await.unwrap().expect("应能查到");
        assert_eq!(found.title, "新会话");

        repo.soft_delete(s.id).await.unwrap();
        assert!(repo.find_by_id(s.id).await.unwrap().is_none(), "软删后应被过滤");
        assert!(repo
            .list_by_project(project_id, None)
            .await
            .unwrap()
            .is_empty());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn sum_tokens_by_project_aggregates_and_skips_soft_deleted(pool: SqlitePool) {
        let project_id = seed_project(&pool).await;
        let repo = SessionRepositoryImpl::new(pool.clone());

        let mut s1 = Session::new(project_id, now_local());
        s1.update_usage(100, 50, 10);
        repo.insert(&mut s1).await.unwrap();
        let mut s2 = Session::new(project_id, now_local());
        s2.update_usage(200, 80, 20);
        repo.insert(&mut s2).await.unwrap();

        let (input, output, count) = repo.sum_tokens_by_project(project_id).await.unwrap();
        assert_eq!((input, output, count), (300, 130, 2));

        repo.soft_delete(s2.id).await.unwrap();
        let (input, output, count) = repo.sum_tokens_by_project(project_id).await.unwrap();
        assert_eq!((input, output, count), (100, 50, 1), "软删会话不应计入聚合");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn message_insert_backfills_id_and_lists_by_seq(pool: SqlitePool) {
        let project_id = seed_project(&pool).await;
        let session_repo = SessionRepositoryImpl::new(pool.clone());
        let msg_repo = MessageRepositoryImpl::new(pool.clone());

        let mut s = Session::new(project_id, now_local());
        session_repo.insert(&mut s).await.unwrap();

        let mut m1 = Message::new(s.id, MessageKind::User, Message::text_payload("你好"), now_local());
        m1.seq = 1;
        msg_repo.insert(&mut m1).await.unwrap();
        assert!(m1.id > 0);
        let mut m2 = Message::new(s.id, MessageKind::Assistant, Message::text_payload("你好！"), now_local());
        m2.seq = 2;
        msg_repo.insert(&mut m2).await.unwrap();

        let list = msg_repo.list_by_session(s.id).await.unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].kind, MessageKind::User);
        assert_eq!(list[0].text().as_deref(), Some("你好"));

        msg_repo.soft_delete_by_session(s.id).await.unwrap();
        assert!(msg_repo.list_by_session(s.id).await.unwrap().is_empty());
    }
}
