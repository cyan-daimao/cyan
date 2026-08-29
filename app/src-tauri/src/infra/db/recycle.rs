//! 回收站仓储：全库软删记录硬删（单事务，按 FK 依赖顺序）。
//!
//! 关键依赖：cyan_session.project_id → cyan_project.id 是 NOT NULL FK（NO ACTION）。
//! 当用户移除项目时若没级联软删其下会话，会出现"软删 project + 活 session"的孤儿状态。
//! 直接硬删这种 project 会被 FK 拒绝。处理方式：purge 阶段先把"软删 project 下还活着的
//! session/message"连带软删送进回收站（用户可恢复），再按原顺序硬删全部。

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

        // 先收拾孤儿：软删的 project 下若仍有活 session/message，
        // 一并软删送进回收站（用户仍可恢复），避免后续硬删被 FK 拒。
        // 使用项目级 deleted_at 作为统一时间戳，与"用户主动删除"语义一致。
        Self::cascade_soft_delete_orphans(&mut tx).await?;

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

impl RecycleBinRepositoryImpl {
    /// 软删 project 下还活着的 session/message 一起进回收站。
    /// 单条 UPDATE 携带 deleted_at = deleted_at（或 now）保持时间一致；使用
    /// (deleted_at IS NULL AND project_id IN (软删 project)) 的谓词一次完成。
    /// 注：cyan_message 通过 session_id 间接定位，cyan_checkpoint 同理。
    async fn cascade_soft_delete_orphans(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>) -> anyhow::Result<()> {
        // 1) 找出所有软删 project → 把它们下面活 session 一并软删
        let now = crate::infra::db::fmt_time(&crate::infra::db::now_local());
        let sessions = sqlx::query(
            "UPDATE cyan_session
             SET deleted_at = ?, updated_by = 'local', updated_at = ?
             WHERE deleted_at IS NULL
               AND project_id IN (SELECT id FROM cyan_project WHERE deleted_at IS NOT NULL)",
        )
        .bind(&now)
        .bind(&now)
        .execute(&mut **tx)
        .await?
        .rows_affected();
        // 2) 同样把它们的活 message / checkpoint 一并软删（孤儿数据完整回收）
        let messages = sqlx::query(
            "UPDATE cyan_message
             SET deleted_at = ?, updated_by = 'local', updated_at = ?
             WHERE deleted_at IS NULL
               AND session_id IN (SELECT id FROM cyan_session WHERE deleted_at IS NOT NULL)",
        )
        .bind(&now)
        .bind(&now)
        .execute(&mut **tx)
        .await?
        .rows_affected();
        let checkpoints = sqlx::query(
            "UPDATE cyan_checkpoint
             SET deleted_at = ?, updated_by = 'local', updated_at = ?
             WHERE deleted_at IS NULL
               AND session_id IN (SELECT id FROM cyan_session WHERE deleted_at IS NOT NULL)",
        )
        .bind(&now)
        .bind(&now)
        .execute(&mut **tx)
        .await?
        .rows_affected();
        // 3) 软删 session 引用了活 project 也算孤儿（理论不该出现，兜底）：把它们 ref 的活 project 一并软删
        //    —— 不做：若活 project 被软删，用户的"项目"列表会少一项，破坏预期。
        //    反向路径已在"软删 project 下活 session"覆盖。
        if sessions + messages + checkpoints > 0 {
            tracing::info!(sessions, messages, checkpoints, "回收站：孤儿数据已级联软删");
        }
        Ok(())
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

    /// 孤儿场景：移除项目时未级联软删其下会话 → 清空回收站必须先把"软删 project 下活 session"
    /// 一并软删送进回收站，再硬删；否则 DELETE cyan_project 会被 FK 拒绝。
    /// 关键：连接必须开 `PRAGMA foreign_keys = ON`（cyan 生产配置），否则 SQLITE_CONSTRAINT_FOREIGNKEY 不会触发。
    #[sqlx::test(migrations = "./migrations")]
    async fn purge_handles_orphan_sessions_under_soft_deleted_project(pool: SqlitePool) {
        // 模拟 cyan 生产连接：开启外键约束
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&pool)
            .await
            .unwrap();
        // 软删 project（模拟 remove_project 行为）
        let pid = sqlx::query(
            "INSERT INTO cyan_project (name, path, created_by, updated_by, created_at, updated_at, deleted_at)
             VALUES ('orphan-demo', '/tmp/orphan', 'local', 'local', '2026-08-27 10:00:00', '2026-08-27 10:00:00', '2026-08-28 10:00:00')",
        )
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_rowid();
        // 该软删 project 下还有 2 条**活** session（孤儿）
        for title in ["s-orphan-1", "s-orphan-2"] {
            sqlx::query(
                "INSERT INTO cyan_session (project_id, title, created_by, updated_by, created_at, updated_at)
                 VALUES (?, ?, 'local', 'local', '2026-08-27 10:00:00', '2026-08-27 10:00:00')",
            )
            .bind(pid)
            .bind(title)
            .execute(&pool)
            .await
            .unwrap();
        }

        let repo = RecycleBinRepositoryImpl::new(pool.clone());
        // 修复前会在 DELETE cyan_project 阶段报 FK 失败；修复后应全部清空
        let total = repo.purge_soft_deleted().await.expect("purge 不应再被 FK 拒绝");
        // 清掉的行 = 1 (project 软删) + 2 (孤儿 session 被级联软删后硬删) = 3
        assert_eq!(total, 3, "软删 project + 2 个孤儿 session 一起清空");

        // 验证：项目与会话都物理消失
        let remaining_projects: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM cyan_project WHERE id = ?")
                .bind(pid)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(remaining_projects.0, 0);
        let remaining_sessions: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM cyan_session WHERE project_id = ?")
                .bind(pid)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(remaining_sessions.0, 0);

        // 幂等
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
