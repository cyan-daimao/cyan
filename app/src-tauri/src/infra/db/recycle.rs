//! 回收站仓储：全库软删记录硬删（单事务，按 FK 依赖顺序）。

use async_trait::async_trait;
use sqlx::SqlitePool;

use crate::domain::session::RecycleBinRepository;

/// 硬删顺序（FK 依赖先行）：message → checkpoint → session → project → perm_rule → plugin → mcp_server → model_config
const PURGE_TABLES: &[&str] = &[
    "cyan_message",
    "cyan_checkpoint",
    "cyan_session",
    "cyan_project",
    "cyan_permission_rule",
    "cyan_plugin",
    "cyan_mcp_server",
    "cyan_model_config",
];

/// 回收站仓储 SQLx 实现
pub struct RecycleBinRepositoryImpl {
    pool: SqlitePool,
}

impl RecycleBinRepositoryImpl {
    /// 构造
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl RecycleBinRepository for RecycleBinRepositoryImpl {
    async fn purge_soft_deleted(&self) -> anyhow::Result<i64> {
        let mut tx = self.pool.begin().await?;
        let mut total: i64 = 0;
        for table in PURGE_TABLES {
            // 表名为本模块常量，无注入风险
            let rows = sqlx::query(&format!("DELETE FROM {table} WHERE deleted_at IS NOT NULL"))
                .execute(&mut *tx)
                .await?
                .rows_affected();
            total += rows as i64;
        }
        tx.commit().await?;
        tracing::info!(total, "回收站清理完成");
        Ok(total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::db::session_repo::{MessageRepositoryImpl, SessionRepositoryImpl};
    use crate::domain::session::{SessionRepository, MessageRepository};

    #[sqlx::test(migrations = "./migrations")]
    async fn purge_hard_deletes_across_tables(pool: SqlitePool) {
        // 种子：项目 + 会话（软删）+ 消息（软删）+ 正常模型配置（不删）
        let pid = sqlx::query(
            "INSERT INTO cyan_project (name, path, created_by, updated_by, created_at, updated_at)
             VALUES ('demo', '/tmp/demo', 'local', 'local', '2026-08-27 10:00:00', '2026-08-27 10:00:00')",
        )
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_rowid();
        let sid = sqlx::query(
            "INSERT INTO cyan_session (project_id, title, created_by, updated_by, created_at, updated_at, deleted_at)
             VALUES (?, 's1', 'local', 'local', '2026-08-27 10:00:00', '2026-08-27 10:00:00', '2026-08-28 10:00:00')",
        )
        .bind(pid)
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_rowid();
        sqlx::query(
            "INSERT INTO cyan_message (session_id, seq, kind, payload, created_by, updated_by, created_at, updated_at, deleted_at)
             VALUES (?, 1, 'user', '{\"text\":\"hi\"}', 'local', 'local', '2026-08-27 10:00:00', '2026-08-27 10:00:00', '2026-08-28 10:00:00')",
        )
        .bind(sid)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO cyan_model_config (name, provider, base_url, api_key, context_window, created_by, updated_by, created_at, updated_at)
             VALUES ('m1', 'p', 'https://x.dev', 'keychain://cyan/model/m1', 128000, 'local', 'local', '2026-08-27 10:00:00', '2026-08-27 10:00:00')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let repo = RecycleBinRepositoryImpl::new(pool.clone());
        let total = repo.purge_soft_deleted().await.unwrap();
        assert_eq!(total, 2, "软删的会话 + 消息共 2 行");

        // 硬删后查无；未软删的模型配置保留
        let session_repo = SessionRepositoryImpl::new(pool.clone());
        assert!(session_repo.list_deleted().await.unwrap().is_empty());
        let remaining: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM cyan_model_config")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(remaining.0, 1);
        // 幂等：再清一次为 0
        assert_eq!(repo.purge_soft_deleted().await.unwrap(), 0);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn restore_roundtrip(pool: SqlitePool) {
        let pid = sqlx::query(
            "INSERT INTO cyan_project (name, path, created_by, updated_by, created_at, updated_at)
             VALUES ('demo', '/tmp/demo2', 'local', 'local', '2026-08-27 10:00:00', '2026-08-27 10:00:00')",
        )
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_rowid();
        let session_repo = SessionRepositoryImpl::new(pool.clone());
        let msg_repo = MessageRepositoryImpl::new(pool.clone());
        let mut s = crate::domain::session::Session::new(pid, crate::infra::db::now_local());
        session_repo.insert(&mut s).await.unwrap();
        let mut m = crate::domain::session::Message::new(
            s.id,
            crate::domain::session::MessageKind::User,
            crate::domain::session::Message::text_payload("hi"),
            crate::infra::db::now_local(),
        );
        m.seq = 1;
        msg_repo.insert(&mut m).await.unwrap();

        // 删除 → 回收站可见
        session_repo.soft_delete(s.id).await.unwrap();
        msg_repo.soft_delete_by_session(s.id).await.unwrap();
        let deleted = session_repo.list_deleted().await.unwrap();
        assert_eq!(deleted.len(), 1);
        assert!(session_repo.find_by_id(s.id).await.unwrap().is_none());
        assert!(msg_repo.list_by_session(s.id).await.unwrap().is_empty());

        // 恢复 → 会话与消息都回来；幂等
        session_repo.restore(s.id).await.unwrap();
        msg_repo.restore_by_session(s.id).await.unwrap();
        assert!(session_repo.find_by_id(s.id).await.unwrap().is_some());
        assert_eq!(msg_repo.list_by_session(s.id).await.unwrap().len(), 1);
        assert!(session_repo.list_deleted().await.unwrap().is_empty());
        session_repo.restore(s.id).await.unwrap();
        msg_repo.restore_by_session(s.id).await.unwrap();
        assert!(session_repo.find_by_id(s.id).await.unwrap().is_some());
    }
}
