//! RecycleService 实现：六类软删对象的列表与恢复（含级联窗口语义）。

use std::sync::Arc;

use async_trait::async_trait;

use crate::application::project_service::ProjectBO;
use crate::application::session_service::SessionBO;
use crate::domain::agent::CheckpointRepository;
use crate::domain::config::{McpRepository, ModelRepository, PermRuleRepository};
use crate::domain::plugin::PluginRepository;
use crate::domain::project::ProjectRepository;
use crate::domain::session::{MessageRepository, SessionRepository};
use crate::error::ServiceError;
use crate::infra::db::fmt_time;

use super::{RecycleBinBO, RecycleKind, RestoreRecycleItemCmd};

/// 回收站服务
#[async_trait]
pub trait RecycleService: Send + Sync {
    /// 回收站全量列表（六类软删记录）
    async fn list_recycle_bin(&self) -> Result<RecycleBinBO, ServiceError>;
    /// 恢复单个对象（幂等；不存在/未删除报友好错误）
    async fn restore_item(&self, cmd: RestoreRecycleItemCmd) -> Result<(), ServiceError>;
}

/// 回收站服务实现
pub struct RecycleServiceImpl {
    session_repo: Arc<dyn SessionRepository>,
    message_repo: Arc<dyn MessageRepository>,
    project_repo: Arc<dyn ProjectRepository>,
    checkpoint_repo: Arc<dyn CheckpointRepository>,
    model_repo: Arc<dyn ModelRepository>,
    mcp_repo: Arc<dyn McpRepository>,
    plugin_repo: Arc<dyn PluginRepository>,
    perm_repo: Arc<dyn PermRuleRepository>,
}

impl RecycleServiceImpl {
    /// 构造
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        session_repo: Arc<dyn SessionRepository>,
        message_repo: Arc<dyn MessageRepository>,
        project_repo: Arc<dyn ProjectRepository>,
        checkpoint_repo: Arc<dyn CheckpointRepository>,
        model_repo: Arc<dyn ModelRepository>,
        mcp_repo: Arc<dyn McpRepository>,
        plugin_repo: Arc<dyn PluginRepository>,
        perm_repo: Arc<dyn PermRuleRepository>,
    ) -> Self {
        Self {
            session_repo,
            message_repo,
            project_repo,
            checkpoint_repo,
            model_repo,
            mcp_repo,
            plugin_repo,
            perm_repo,
        }
    }

    /// 友好错误：对象不在回收站
    fn not_in_recycle_bin(kind: &str, id: i64) -> ServiceError {
        ServiceError::not_found(format!("回收站中不存在该{kind}（id={id}），或已被彻底清理"))
    }

    async fn restore_session(&self, id: i64) -> Result<(), ServiceError> {
        let deleted = self.session_repo.list_deleted().await?;
        let Some(session) = deleted.iter().find(|s| s.id == id) else {
            // 幂等：仍在活跃列表（已恢复过）视为成功
            if self.session_repo.find_by_id(id).await?.is_some() {
                return Ok(());
            }
            return Err(Self::not_in_recycle_bin("会话", id));
        };
        self.session_repo.restore(id).await?;
        self.message_repo.restore_by_session(id).await?;
        // 向上级联：所属项目仍是软删状态时一并恢复（仅项目行本身）
        if self.project_repo.find_by_id(session.project_id).await?.is_none()
            && self
                .project_repo
                .find_by_id_include_deleted(session.project_id)
                .await?
                .is_some()
        {
            self.project_repo.restore(session.project_id).await?;
        }
        Ok(())
    }

    async fn restore_project(&self, id: i64) -> Result<(), ServiceError> {
        let Some(project) = self.project_repo.find_by_id_include_deleted(id).await? else {
            return Err(Self::not_in_recycle_bin("项目", id));
        };
        // 幂等：未被删除
        let Some(window) = project.deleted_at else {
            return Ok(());
        };
        let window = fmt_time(&window);
        self.project_repo.restore(id).await?;
        // 级联恢复「随项目一起删的」（同窗口时间戳）：会话/消息/checkpoint/项目级规则
        self.session_repo.restore_by_project_window(id, &window).await?;
        self.message_repo.restore_by_project_window(id, &window).await?;
        self.checkpoint_repo.restore_by_project_window(id, &window).await?;
        self.perm_repo.restore_project_rules_window(id, &window).await?;
        Ok(())
    }
}

#[async_trait]
impl RecycleService for RecycleServiceImpl {
    async fn list_recycle_bin(&self) -> Result<RecycleBinBO, ServiceError> {
        let mut bin = RecycleBinBO::default();
        // 会话：带所属项目名称/路径（项目可能也被软删，找不到给空串）
        for s in self.session_repo.list_deleted().await? {
            let mut bo = SessionBO::from(s);
            if let Ok(Some(p)) = self.project_repo.find_by_id_include_deleted(bo.project_id).await
            {
                bo.project_name = p.name;
                bo.project_path = p.path;
            }
            bin.sessions.push(bo);
        }
        bin.projects = self
            .project_repo
            .list_deleted()
            .await?
            .into_iter()
            .map(ProjectBO::from)
            .collect();
        bin.models = self
            .model_repo
            .list_deleted()
            .await?
            .into_iter()
            .map(|m| crate::application::config_service::ModelBO::from_domain(m, "****".into()))
            .collect();
        bin.mcp_servers = self
            .mcp_repo
            .list_deleted()
            .await?
            .into_iter()
            .map(crate::application::config_service::McpServerBO::from)
            .collect();
        bin.plugins = self
            .plugin_repo
            .list_deleted()
            .await?
            .into_iter()
            .map(crate::application::plugin_service::PluginBO::from)
            .collect();
        bin.perm_rules = self
            .perm_repo
            .list_deleted()
            .await?
            .into_iter()
            .map(crate::application::config_service::PermRuleBO::from)
            .collect();
        Ok(bin)
    }

    async fn restore_item(&self, cmd: RestoreRecycleItemCmd) -> Result<(), ServiceError> {
        match RecycleKind::parse(&cmd.kind)? {
            RecycleKind::Session => self.restore_session(cmd.id).await,
            RecycleKind::Project => self.restore_project(cmd.id).await,
            RecycleKind::Model => {
                let deleted = self.model_repo.list_deleted().await?;
                if deleted.iter().any(|m| m.id == cmd.id) {
                    self.model_repo.restore(cmd.id).await?;
                    return Ok(());
                }
                if self.model_repo.find_by_id(cmd.id).await?.is_some() {
                    return Ok(()); // 幂等
                }
                Err(Self::not_in_recycle_bin("模型", cmd.id))
            }
            RecycleKind::Mcp => {
                let deleted = self.mcp_repo.list_deleted().await?;
                if deleted.iter().any(|s| s.id == cmd.id) {
                    self.mcp_repo.restore(cmd.id).await?;
                    return Ok(());
                }
                if self.mcp_repo.list().await?.iter().any(|s| s.id == cmd.id) {
                    return Ok(()); // 幂等
                }
                Err(Self::not_in_recycle_bin("MCP 服务器", cmd.id))
            }
            RecycleKind::Plugin => {
                let deleted = self.plugin_repo.list_deleted().await?;
                if deleted.iter().any(|p| p.id == cmd.id) {
                    // 仅恢复记录，保持 disabled 待用户手动启用
                    self.plugin_repo.restore(cmd.id).await?;
                    return Ok(());
                }
                if self.plugin_repo.find_by_id(cmd.id).await?.is_some() {
                    return Ok(()); // 幂等
                }
                Err(Self::not_in_recycle_bin("插件", cmd.id))
            }
            RecycleKind::PermRule => {
                let deleted = self.perm_repo.list_deleted().await?;
                if deleted.iter().any(|r| r.id == cmd.id) {
                    self.perm_repo.restore(cmd.id).await?;
                    return Ok(());
                }
                if self.perm_repo.find_by_id(cmd.id).await?.is_some() {
                    return Ok(()); // 幂等
                }
                Err(Self::not_in_recycle_bin("权限规则", cmd.id))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::project_service::{ProjectService, ProjectServiceImpl, RemoveProjectCmd};
    use crate::domain::session::RecycleBinRepository;
    use crate::infra::db::checkpoint_repo::CheckpointRepositoryImpl;
    use crate::infra::db::mcp_repo::McpRepositoryImpl;
    use crate::infra::db::model_repo::ModelRepositoryImpl;
    use crate::infra::db::perm_rule_repo::PermRuleRepositoryImpl;
    use crate::infra::db::plugin_repo::PluginRepositoryImpl;
    use crate::infra::db::project_repo::ProjectRepositoryImpl;
    use crate::infra::db::recycle::RecycleBinRepositoryImpl;
    use crate::infra::db::session_repo::{MessageRepositoryImpl, SessionRepositoryImpl};
    use sqlx::SqlitePool;

    fn svc(pool: &SqlitePool) -> RecycleServiceImpl {
        RecycleServiceImpl::new(
            Arc::new(SessionRepositoryImpl::new(pool.clone())),
            Arc::new(MessageRepositoryImpl::new(pool.clone())),
            Arc::new(ProjectRepositoryImpl::new(pool.clone())),
            Arc::new(CheckpointRepositoryImpl::new(pool.clone())),
            Arc::new(ModelRepositoryImpl::new(pool.clone())),
            Arc::new(McpRepositoryImpl::new(pool.clone())),
            Arc::new(PluginRepositoryImpl::new(pool.clone())),
            Arc::new(PermRuleRepositoryImpl::new(pool.clone())),
        )
    }

    async fn seed_project(pool: &SqlitePool, path: &str) -> i64 {
        sqlx::query(
            "INSERT INTO cyan_project (name, path, created_by, updated_by, created_at, updated_at)
             VALUES ('demo', ?, 'local', 'local', '2026-08-30 10:00:00', '2026-08-30 10:00:00')",
        )
        .bind(path)
        .execute(pool)
        .await
        .unwrap()
        .last_insert_rowid()
    }

    async fn seed_session(pool: &SqlitePool, pid: i64, title: &str) -> i64 {
        sqlx::query(
            "INSERT INTO cyan_session (project_id, title, created_by, updated_by, created_at, updated_at)
             VALUES (?, ?, 'local', 'local', '2026-08-30 10:00:00', '2026-08-30 10:00:00')",
        )
        .bind(pid)
        .bind(title)
        .execute(pool)
        .await
        .unwrap()
        .last_insert_rowid()
    }

    async fn seed_message(pool: &SqlitePool, sid: i64, seq: i64) {
        sqlx::query(
            "INSERT INTO cyan_message (session_id, seq, kind, payload, created_by, updated_by, created_at, updated_at)
             VALUES (?, ?, 'user', '{\"text\":\"hi\"}', 'local', 'local', '2026-08-30 10:00:00', '2026-08-30 10:00:00')",
        )
        .bind(sid)
        .bind(seq)
        .execute(pool)
        .await
        .unwrap();
    }

    /// 造全类软删种子：模型/MCP/插件/规则各一条软删
    async fn seed_deleted_objects(pool: &SqlitePool) -> (i64, i64, i64, i64) {
        let ts = "2026-08-30 12:00:00";
        let model_id = sqlx::query(
            "INSERT INTO cyan_model_config (name, provider, base_url, api_key, context_window, created_by, updated_by, created_at, updated_at, deleted_at)
             VALUES ('m1', 'p', 'https://x.dev', 'ref', 128000, 'local', 'local', '2026-08-30 10:00:00', '2026-08-30 10:00:00', ?)",
        )
        .bind(ts)
        .execute(pool).await.unwrap().last_insert_rowid();
        let mcp_id = sqlx::query(
            "INSERT INTO cyan_mcp_server (name, command, created_by, updated_by, created_at, updated_at, deleted_at)
             VALUES ('mcp1', 'npx x', 'local', 'local', '2026-08-30 10:00:00', '2026-08-30 10:00:00', ?)",
        )
        .bind(ts)
        .execute(pool).await.unwrap().last_insert_rowid();
        let plugin_id = sqlx::query(
            "INSERT INTO cyan_plugin (name, version, status, created_by, updated_by, created_at, updated_at, deleted_at)
             VALUES ('p1', '1.0.0', 'disabled', 'local', 'local', '2026-08-30 10:00:00', '2026-08-30 10:00:00', ?)",
        )
        .bind(ts)
        .execute(pool).await.unwrap().last_insert_rowid();
        let rule_id = sqlx::query(
            "INSERT INTO cyan_permission_rule (tool, pattern, action, sort, created_by, updated_by, created_at, updated_at, deleted_at)
             VALUES ('Bash', 'cargo *', 'allow', 0, 'local', 'local', '2026-08-30 10:00:00', '2026-08-30 10:00:00', ?)",
        )
        .bind(ts)
        .execute(pool).await.unwrap().last_insert_rowid();
        (model_id, mcp_id, plugin_id, rule_id)
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn list_and_restore_each_kind(pool: SqlitePool) {
        let svc = svc(&pool);
        let (model_id, mcp_id, plugin_id, rule_id) = seed_deleted_objects(&pool).await;

        let bin = svc.list_recycle_bin().await.unwrap();
        assert_eq!(bin.models.len(), 1);
        assert_eq!(bin.mcp_servers.len(), 1);
        assert_eq!(bin.plugins.len(), 1);
        assert_eq!(bin.perm_rules.len(), 1);
        assert!(bin.models[0].deleted_at.is_some(), "应带 deletedAt");

        for (kind, id) in [
            ("model", model_id),
            ("mcp", mcp_id),
            ("plugin", plugin_id),
            ("permRule", rule_id),
        ] {
            svc.restore_item(RestoreRecycleItemCmd {
                kind: kind.into(),
                id,
            })
            .await
            .unwrap();
            // 幂等：重复恢复不报错
            svc.restore_item(RestoreRecycleItemCmd {
                kind: kind.into(),
                id,
            })
            .await
            .unwrap();
        }
        let bin = svc.list_recycle_bin().await.unwrap();
        assert!(bin.models.is_empty() && bin.mcp_servers.is_empty() && bin.plugins.is_empty() && bin.perm_rules.is_empty());
        // 插件恢复后保持 disabled
        let plugin = PluginRepositoryImpl::new(pool.clone())
            .find_by_id(plugin_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(plugin.status.as_str(), "disabled");
        // 不存在的 id → 友好错误
        let err = svc
            .restore_item(RestoreRecycleItemCmd {
                kind: "model".into(),
                id: 9999,
            })
            .await
            .unwrap_err();
        assert_eq!(err.code, 2002);
        // 非法 kind → validation
        assert!(svc
            .restore_item(RestoreRecycleItemCmd {
                kind: "nope".into(),
                id: 1,
            })
            .await
            .is_err());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn session_restore_bring_back_deleted_project(pool: SqlitePool) {
        let svc = svc(&pool);
        let pid = seed_project(&pool, "/tmp/rp1").await;
        let sid = seed_session(&pool, pid, "s1").await;
        seed_message(&pool, sid, 1).await;
        // 项目软删（会话仍活）——模拟项目被单独移除的边界场景
        ProjectRepositoryImpl::new(pool.clone()).soft_delete(pid).await.unwrap();
        SessionRepositoryImpl::new(pool.clone()).soft_delete(sid).await.unwrap();
        MessageRepositoryImpl::new(pool.clone()).soft_delete_by_session(sid).await.unwrap();

        let bin = svc.list_recycle_bin().await.unwrap();
        assert_eq!(bin.sessions.len(), 1);
        assert_eq!(bin.sessions[0].project_name, "demo", "回收站会话带项目名称（含软删项目）");

        // 恢复会话 → 项目自动一并恢复
        svc.restore_item(RestoreRecycleItemCmd {
            kind: "session".into(),
            id: sid,
        })
        .await
        .unwrap();
        assert!(ProjectRepositoryImpl::new(pool.clone())
            .find_by_id(pid)
            .await
            .unwrap()
            .is_some());
        assert!(SessionRepositoryImpl::new(pool.clone())
            .find_by_id(sid)
            .await
            .unwrap()
            .is_some());
        assert_eq!(
            MessageRepositoryImpl::new(pool.clone())
                .list_by_session(sid)
                .await
                .unwrap()
                .len(),
            1,
            "消息随会话恢复"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn project_restore_only_brings_back_same_window_sessions(pool: SqlitePool) {
        let svc = svc(&pool);
        let pid = seed_project(&pool, "/tmp/rp2").await;
        let s_alone = seed_session(&pool, pid, "单独删除").await;
        let s_cascade = seed_session(&pool, pid, "随项目删除").await;
        seed_message(&pool, s_alone, 1).await;
        seed_message(&pool, s_cascade, 1).await;

        // 用户先单独删 s_alone（自己的时间戳）
        SessionRepositoryImpl::new(pool.clone()).soft_delete(s_alone).await.unwrap();
        MessageRepositoryImpl::new(pool.clone()).soft_delete_by_session(s_alone).await.unwrap();
        // 时间戳秒级精度：隔开一秒确保两个删除窗口不同
        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;

        // 再经 remove_project 级联删除（统一窗口时间戳）
        let project_service = ProjectServiceImpl::new(
            Arc::new(ProjectRepositoryImpl::new(pool.clone())),
            Arc::new(SessionRepositoryImpl::new(pool.clone())),
            Arc::new(MessageRepositoryImpl::new(pool.clone())),
            Arc::new(CheckpointRepositoryImpl::new(pool.clone())),
            Arc::new(PermRuleRepositoryImpl::new(pool.clone())),
        );
        project_service
            .remove_project(RemoveProjectCmd {
                path: "/tmp/rp2".into(),
            })
            .await
            .unwrap();

        // 恢复项目：只带回同窗删除的 s_cascade，s_alone 保持删除
        svc.restore_item(RestoreRecycleItemCmd {
            kind: "project".into(),
            id: pid,
        })
        .await
        .unwrap();
        let session_repo = SessionRepositoryImpl::new(pool.clone());
        assert!(session_repo.find_by_id(s_cascade).await.unwrap().is_some(), "同窗会话应恢复");
        assert!(session_repo.find_by_id(s_alone).await.unwrap().is_none(), "单独删除的会话保持删除");
        // 同窗消息也恢复
        assert_eq!(
            MessageRepositoryImpl::new(pool.clone())
                .list_by_session(s_cascade)
                .await
                .unwrap()
                .len(),
            1
        );
        // 幂等：重复恢复项目
        svc.restore_item(RestoreRecycleItemCmd {
            kind: "project".into(),
            id: pid,
        })
        .await
        .unwrap();
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn purge_empties_recycle_bin(pool: SqlitePool) {
        let svc = svc(&pool);
        seed_deleted_objects(&pool).await;
        let pid = seed_project(&pool, "/tmp/rp3").await;
        let sid = seed_session(&pool, pid, "s").await;
        SessionRepositoryImpl::new(pool.clone()).soft_delete(sid).await.unwrap();
        ProjectRepositoryImpl::new(pool.clone()).soft_delete(pid).await.unwrap();

        let bin = svc.list_recycle_bin().await.unwrap();
        assert!(!bin.sessions.is_empty() && !bin.projects.is_empty() && !bin.models.is_empty());

        let purged = RecycleBinRepositoryImpl::new(pool.clone())
            .purge_soft_deleted()
            .await
            .unwrap();
        assert!(purged >= 6);
        let bin = svc.list_recycle_bin().await.unwrap();
        assert!(bin.sessions.is_empty() && bin.projects.is_empty() && bin.models.is_empty()
            && bin.mcp_servers.is_empty() && bin.plugins.is_empty() && bin.perm_rules.is_empty());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn remove_project_uses_unified_window_timestamp(pool: SqlitePool) {
        // remove_project 级联：项目行与子对象的 deleted_at 必须一致（窗口恢复依赖）
        let pid = seed_project(&pool, "/tmp/rp4").await;
        let sid = seed_session(&pool, pid, "s").await;
        let project_service = ProjectServiceImpl::new(
            Arc::new(ProjectRepositoryImpl::new(pool.clone())),
            Arc::new(SessionRepositoryImpl::new(pool.clone())),
            Arc::new(MessageRepositoryImpl::new(pool.clone())),
            Arc::new(CheckpointRepositoryImpl::new(pool.clone())),
            Arc::new(PermRuleRepositoryImpl::new(pool.clone())),
        );
        project_service
            .remove_project(RemoveProjectCmd {
                path: "/tmp/rp4".into(),
            })
            .await
            .unwrap();
        let ts_of = |table: &str, id: i64| {
            let pool = pool.clone();
            let table = table.to_string();
            async move {
                sqlx::query_as::<_, (Option<String>,)>(&format!(
                    "SELECT deleted_at FROM {table} WHERE id = ?"
                ))
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap()
                .0
            }
        };
        let p_ts = ts_of("cyan_project", pid).await;
        let s_ts = ts_of("cyan_session", sid).await;
        assert_eq!(p_ts, s_ts, "级联删除时间戳必须统一");
        assert!(p_ts.is_some());
    }
}
