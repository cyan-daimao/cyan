//! SessionService 实现：会话与消息编排（Repository 以 Arc<dyn> 注入）。

use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::config::ModelRepository;
use crate::domain::project::ProjectRepository;
use crate::domain::agent::CheckpointRepository;
use crate::domain::session::{
    Message, MessageKind, MessageRepository, RecycleBinRepository, Session, SessionRepository,
};
use crate::error::ServiceError;
use crate::infra::db::now_local;

use super::{
    AppendMessageCmd, ClearSessionCmd, CreateSessionCmd, DeleteSessionCmd, EditMessageCmd,
    GetSessionQuery, ListSessionQuery, MessageBO, ProjectTokenUsageBO, ProjectTokenUsageQuery,
    RenameSessionCmd, RestoreSessionCmd, SessionBO, SessionSummaryBO, SetSessionModelCmd,
};

/// 会话服务
#[async_trait]
pub trait SessionService: Send + Sync {
    /// 会话列表/搜索
    async fn list_sessions(&self, query: ListSessionQuery) -> Result<Vec<SessionSummaryBO>, ServiceError>;
    /// 打开会话（含全部消息）
    async fn get_session(&self, query: GetSessionQuery) -> Result<SessionBO, ServiceError>;
    /// 新建会话
    async fn create_session(&self, cmd: CreateSessionCmd) -> Result<SessionBO, ServiceError>;
    /// 删除会话（软删会话与消息）
    async fn delete_session(&self, cmd: DeleteSessionCmd) -> Result<(), ServiceError>;
    /// 追加消息（AgentService 复用）
    async fn append_message(&self, cmd: AppendMessageCmd) -> Result<MessageBO, ServiceError>;
    /// 项目级 token 用量聚合
    async fn token_usage(
        &self,
        query: ProjectTokenUsageQuery,
    ) -> Result<ProjectTokenUsageBO, ServiceError>;
    /// 回收站：软删会话列表（带所属项目名称/路径）
    async fn list_deleted_sessions(&self) -> Result<Vec<SessionBO>, ServiceError>;
    /// 恢复会话 + 该会话全部软删消息（幂等）
    async fn restore_session(&self, cmd: RestoreSessionCmd) -> Result<(), ServiceError>;
    /// 清空回收站：全库软删记录硬删，返回总删除行数
    async fn purge_recycle_bin(&self) -> Result<i64, ServiceError>;
    /// 编辑用户消息（编辑即截断：更新 payload 文本 + 物理删除后续消息），返回更新后的完整会话
    async fn edit_message(&self, cmd: EditMessageCmd) -> Result<SessionBO, ServiceError>;
    /// 设置会话级模型偏好（空串 = 清除跟随全局；幂等）
    async fn set_session_model(&self, cmd: SetSessionModelCmd) -> Result<(), ServiceError>;
    /// 重命名会话（trim 后 1..=80 字符；幂等：同值不写盘）
    async fn rename_session(&self, cmd: RenameSessionCmd) -> Result<(), ServiceError>;
    /// 清空会话上下文（/clear）：硬删全部消息 + checkpoint，统计归零；空会话幂等
    async fn clear_session(&self, cmd: ClearSessionCmd) -> Result<u64, ServiceError>;
}

/// 会话服务实现
pub struct SessionServiceImpl {
    session_repo: Arc<dyn SessionRepository>,
    message_repo: Arc<dyn MessageRepository>,
    project_repo: Arc<dyn ProjectRepository>,
    recycle_repo: Arc<dyn RecycleBinRepository>,
    model_repo: Arc<dyn ModelRepository>,
    checkpoint_repo: Arc<dyn CheckpointRepository>,
}

impl SessionServiceImpl {
    /// 构造
    pub fn new(
        session_repo: Arc<dyn SessionRepository>,
        message_repo: Arc<dyn MessageRepository>,
        project_repo: Arc<dyn ProjectRepository>,
        recycle_repo: Arc<dyn RecycleBinRepository>,
        model_repo: Arc<dyn ModelRepository>,
        checkpoint_repo: Arc<dyn CheckpointRepository>,
    ) -> Self {
        Self {
            session_repo,
            message_repo,
            project_repo,
            recycle_repo,
            model_repo,
            checkpoint_repo,
        }
    }

    /// 加载会话并装配消息
    async fn load_with_messages(&self, session_id: i64) -> Result<Session, ServiceError> {
        let mut session = self
            .session_repo
            .find_by_id(session_id)
            .await?
            .ok_or_else(|| ServiceError::not_found(format!("会话不存在：{session_id}")))?;
        session.messages = self.message_repo.list_by_session(session_id).await?;
        Ok(session)
    }
}

#[async_trait]
impl SessionService for SessionServiceImpl {
    async fn list_sessions(&self, query: ListSessionQuery) -> Result<Vec<SessionSummaryBO>, ServiceError> {
        let project = self
            .project_repo
            .find_by_path(&query.project_path)
            .await?
            .ok_or_else(|| ServiceError::not_found("项目未注册，请先打开项目"))?;
        let sessions = self
            .session_repo
            .list_by_project(project.id, query.keyword.as_deref())
            .await?;
        Ok(sessions.into_iter().map(SessionSummaryBO::from).collect())
    }

    async fn get_session(&self, query: GetSessionQuery) -> Result<SessionBO, ServiceError> {
        let session = self.load_with_messages(query.session_id).await?;
        Ok(SessionBO::from(session))
    }

    async fn create_session(&self, cmd: CreateSessionCmd) -> Result<SessionBO, ServiceError> {
        let project = self
            .project_repo
            .find_by_path(&cmd.project_path)
            .await?
            .ok_or_else(|| ServiceError::not_found("项目未注册，请先打开项目"))?;
        let mut session = Session::new(project.id, now_local());
        self.session_repo.insert(&mut session).await?;
        Ok(SessionBO::from(session))
    }

    async fn delete_session(&self, cmd: DeleteSessionCmd) -> Result<(), ServiceError> {
        self.session_repo.soft_delete(cmd.session_id).await?;
        self.message_repo.soft_delete_by_session(cmd.session_id).await?;
        Ok(())
    }

    async fn append_message(&self, cmd: AppendMessageCmd) -> Result<MessageBO, ServiceError> {
        let mut session = self.load_with_messages(cmd.session_id).await?;
        let kind = MessageKind::parse(&cmd.kind)?;
        let message = Message::new(cmd.session_id, kind, cmd.payload, now_local());
        let mut message = session.append_message(message).clone();
        self.message_repo.insert(&mut message).await?;
        Ok(MessageBO::from(message))
    }

    async fn token_usage(
        &self,
        query: ProjectTokenUsageQuery,
    ) -> Result<ProjectTokenUsageBO, ServiceError> {
        let project = self
            .project_repo
            .find_by_path(&query.project_path)
            .await?
            .ok_or_else(|| ServiceError::not_found("项目未注册，请先打开项目"))?;
        let (input_tokens, output_tokens, session_count) =
            self.session_repo.sum_tokens_by_project(project.id).await?;
        Ok(ProjectTokenUsageBO {
            input_tokens,
            output_tokens,
            session_count,
        })
    }

    async fn list_deleted_sessions(&self) -> Result<Vec<SessionBO>, ServiceError> {
        let sessions = self.session_repo.list_deleted().await?;
        let mut bos = Vec::with_capacity(sessions.len());
        for s in sessions {
            let mut bo = SessionBO::from(s);
            // 项目可能也被软删：含软删查询，找不到给空串
            if let Ok(Some(p)) = self.project_repo.find_by_id_include_deleted(bo.project_id).await
            {
                bo.project_name = p.name;
                bo.project_path = p.path;
            }
            bos.push(bo);
        }
        Ok(bos)
    }

    async fn restore_session(&self, cmd: RestoreSessionCmd) -> Result<(), ServiceError> {
        self.session_repo.restore(cmd.id).await?;
        self.message_repo.restore_by_session(cmd.id).await?;
        Ok(())
    }

    async fn purge_recycle_bin(&self) -> Result<i64, ServiceError> {
        Ok(self.recycle_repo.purge_soft_deleted().await?)
    }

    async fn edit_message(&self, cmd: EditMessageCmd) -> Result<SessionBO, ServiceError> {
        let text = cmd.text.trim();
        if text.is_empty() {
            return Err(ServiceError::validation("消息文本不能为空"));
        }
        let msg = self
            .message_repo
            .find_by_id(cmd.id)
            .await?
            .ok_or_else(|| ServiceError::not_found(format!("消息不存在：{}", cmd.id)))?;
        if msg.kind != MessageKind::User {
            return Err(ServiceError::validation("仅支持编辑用户消息"));
        }
        // 替换 payload 的 text 键，其余键（thinking/toolCalls 等）原样保留
        let mut payload: serde_json::Value = serde_json::from_str(&msg.payload)
            .map_err(|_| ServiceError::validation("消息载荷不是合法 JSON"))?;
        let obj = payload
            .as_object_mut()
            .ok_or_else(|| ServiceError::validation("消息载荷不是 JSON 对象"))?;
        obj.insert("text".to_string(), serde_json::Value::String(text.to_string()));
        self.message_repo
            .update_payload(msg.id, &payload.to_string())
            .await?;
        // 编辑即截断：物理删除同会话 seq 更大的消息
        let removed = self
            .message_repo
            .hard_delete_after(msg.session_id, msg.seq)
            .await?;
        tracing::info!(message_id = msg.id, removed, "消息已编辑并截断后续");
        let session = self.load_with_messages(msg.session_id).await?;
        Ok(SessionBO::from(session))
    }

    async fn set_session_model(&self, cmd: SetSessionModelCmd) -> Result<(), ServiceError> {
        // 会话不存在 → not_found
        self.session_repo
            .find_by_id(cmd.session_id)
            .await?
            .ok_or_else(|| ServiceError::not_found(format!("会话不存在：{}", cmd.session_id)))?;
        let model = cmd.model.trim();
        let preference = if model.is_empty() {
            None // 空串 = 清除偏好（跟随全局）
        } else {
            // 校验模型配置存在（不限启用状态）
            if self.model_repo.find_by_name(model).await?.is_none() {
                return Err(ServiceError::validation(format!("模型配置不存在：{model}")));
            }
            Some(model)
        };
        self.session_repo
            .set_preferred_model(cmd.session_id, preference)
            .await?;
        Ok(())
    }

    async fn rename_session(&self, cmd: RenameSessionCmd) -> Result<(), ServiceError> {
        let mut session = self
            .session_repo
            .find_by_id(cmd.id)
            .await?
            .ok_or_else(|| ServiceError::not_found(format!("会话不存在：{}", cmd.id)))?;
        // domain 校验：空/超长 → validation；同值不写盘（幂等）
        session
            .rename_title(&cmd.title, now_local())
            .map_err(ServiceError::validation)?;
        // 复用 update 写盘（updated_at 已在 rename_title 内刷新到 now）
        // 但 update 自己也会写一次 updated_at（now_local 二次取值）— 与重命名语义一致，可接受
        self.session_repo.update(&session).await?;
        tracing::info!(session_id = session.id, title = %session.title, "会话已重命名");
        Ok(())
    }

    async fn clear_session(&self, cmd: ClearSessionCmd) -> Result<u64, ServiceError> {
        // 会话不存在 → not_found
        self.session_repo
            .find_by_id(cmd.session_id)
            .await?
            .ok_or_else(|| ServiceError::not_found(format!("会话不存在：{}", cmd.session_id)))?;
        // 物理删除全部消息与 checkpoint（含软删残留），/clear 语义：不可恢复
        let removed = self
            .message_repo
            .hard_delete_by_session(cmd.session_id)
            .await?;
        self.checkpoint_repo
            .hard_delete_by_session(cmd.session_id)
            .await?;
        // token/ctx 统计归零
        self.session_repo.reset_usage(cmd.session_id).await?;
        tracing::info!(session_id = cmd.session_id, removed, "会话上下文已清空");
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::db::model_repo::ModelRepositoryImpl;
    use crate::infra::db::project_repo::ProjectRepositoryImpl;
    use crate::infra::db::recycle::RecycleBinRepositoryImpl;
    use crate::infra::db::session_repo::{MessageRepositoryImpl, SessionRepositoryImpl};
    use sqlx::SqlitePool;

    fn svc(pool: &SqlitePool) -> SessionServiceImpl {
        SessionServiceImpl::new(
            Arc::new(SessionRepositoryImpl::new(pool.clone())),
            Arc::new(MessageRepositoryImpl::new(pool.clone())),
            Arc::new(ProjectRepositoryImpl::new(pool.clone())),
            Arc::new(RecycleBinRepositoryImpl::new(pool.clone())),
            Arc::new(ModelRepositoryImpl::new(pool.clone())),
            Arc::new(crate::infra::db::checkpoint_repo::CheckpointRepositoryImpl::new(pool.clone())),
        )
    }

    async fn seed_session_with_messages(pool: &SqlitePool) -> (i64, Vec<i64>) {
        let pid = sqlx::query(
            "INSERT INTO cyan_project (name, path, created_by, updated_by, created_at, updated_at)
             VALUES ('demo', '/tmp/demo', 'local', 'local', '2026-08-27 10:00:00', '2026-08-27 10:00:00')",
        )
        .execute(pool)
        .await
        .unwrap()
        .last_insert_rowid();
        let sid = sqlx::query(
            "INSERT INTO cyan_session (project_id, title, created_by, updated_by, created_at, updated_at)
             VALUES (?, 's1', 'local', 'local', '2026-08-27 10:00:00', '2026-08-27 10:00:00')",
        )
        .bind(pid)
        .execute(pool)
        .await
        .unwrap()
        .last_insert_rowid();
        let payloads = [
            ("user", r#"{"text":"问题1"}"#),
            ("assistant", r#"{"text":"回答1","thinking":"思考内容","toolCalls":[{"callId":"c1","tool":"Read"}]}"#),
            ("user", r#"{"text":"问题2"}"#),
            ("assistant", r#"{"text":"回答2"}"#),
        ];
        let mut ids = Vec::new();
        for (i, (kind, payload)) in payloads.iter().enumerate() {
            let id = sqlx::query(
                "INSERT INTO cyan_message (session_id, seq, kind, payload, created_by, updated_by, created_at, updated_at)
                 VALUES (?, ?, ?, ?, 'local', 'local', '2026-08-27 10:00:00', '2026-08-27 10:00:00')",
            )
            .bind(sid)
            .bind((i + 1) as i64)
            .bind(kind)
            .bind(payload)
            .execute(pool)
            .await
            .unwrap()
            .last_insert_rowid();
            ids.push(id);
        }
        (sid, ids)
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn edit_message_updates_text_and_truncates(pool: SqlitePool) {
        let svc = svc(&pool);
        let (sid, ids) = seed_session_with_messages(&pool).await;

        // 编辑第 3 条（user「问题2」）
        let bo = svc
            .edit_message(EditMessageCmd {
                id: ids[2],
                text: "改后的问题".into(),
            })
            .await
            .unwrap();

        // 文本更新
        let msg_repo = MessageRepositoryImpl::new(pool.clone());
        let edited = msg_repo.find_by_id(ids[2]).await.unwrap().unwrap();
        let v: serde_json::Value = serde_json::from_str(&edited.payload).unwrap();
        assert_eq!(v["text"], "改后的问题");

        // 第 4 条物理消失（连同软删记录也查无）
        let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM cyan_message WHERE session_id = ?")
            .bind(sid)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(total.0, 3, "seq 更大的消息应物理删除");

        // 返回值为截断后的完整会话
        assert_eq!(bo.messages.len(), 3);
        assert_eq!(bo.messages[2].payload, edited.payload);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn edit_message_rejects_empty_text_missing_id_and_non_user(pool: SqlitePool) {
        let svc = svc(&pool);
        let (_, ids) = seed_session_with_messages(&pool).await;

        let err = svc
            .edit_message(EditMessageCmd {
                id: ids[0],
                text: "   ".into(),
            })
            .await
            .unwrap_err();
        assert_eq!(err.code, 1001);

        let err = svc
            .edit_message(EditMessageCmd {
                id: 9999,
                text: "x".into(),
            })
            .await
            .unwrap_err();
        assert_eq!(err.code, 2002);

        // assistant 消息不可编辑
        let err = svc
            .edit_message(EditMessageCmd {
                id: ids[1],
                text: "改回答".into(),
            })
            .await
            .unwrap_err();
        assert_eq!(err.code, 1001);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn set_session_model_roundtrip(pool: SqlitePool) {
        let svc = svc(&pool);
        let (sid, _) = seed_session_with_messages(&pool).await;
        // 种子模型配置（disabled 状态也允许设为偏好）
        sqlx::query(
            "INSERT INTO cyan_model_config (name, provider, base_url, api_key, context_window, status, created_by, updated_by, created_at, updated_at)
             VALUES ('kimi', 'moonshot', 'https://api.x.dev', 'keychain://cyan/model/kimi', 128000, 'disabled', 'local', 'local', '2026-08-27 10:00:00', '2026-08-27 10:00:00')",
        )
        .execute(&pool)
        .await
        .unwrap();

        // 设置 → get_session 带回
        svc.set_session_model(SetSessionModelCmd {
            session_id: sid,
            model: "kimi".into(),
        })
        .await
        .unwrap();
        let bo = svc.get_session(GetSessionQuery { session_id: sid }).await.unwrap();
        assert_eq!(bo.preferred_model.as_deref(), Some("kimi"));

        // 幂等：重复设置同值
        svc.set_session_model(SetSessionModelCmd {
            session_id: sid,
            model: "kimi".into(),
        })
        .await
        .unwrap();

        // 清空（trim 后空串）→ 跟随全局
        svc.set_session_model(SetSessionModelCmd {
            session_id: sid,
            model: "  ".into(),
        })
        .await
        .unwrap();
        let bo = svc.get_session(GetSessionQuery { session_id: sid }).await.unwrap();
        assert_eq!(bo.preferred_model, None);

        // 不存在的模型 → validation
        let err = svc
            .set_session_model(SetSessionModelCmd {
                session_id: sid,
                model: "ghost".into(),
            })
            .await
            .unwrap_err();
        assert_eq!(err.code, 1001);
        assert!(err.message.contains("模型配置不存在：ghost"));

        // 不存在的会话 → not_found
        let err = svc
            .set_session_model(SetSessionModelCmd {
                session_id: 9999,
                model: "kimi".into(),
            })
            .await
            .unwrap_err();
        assert_eq!(err.code, 2002);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn clear_session_hard_deletes_messages_and_resets_usage(pool: SqlitePool) {
        let svc = svc(&pool);
        let (sid, _) = seed_session_with_messages(&pool).await;

        // 模拟有 token 统计
        sqlx::query(
            "UPDATE cyan_session SET input_tokens = 1500, output_tokens = 700, ctx_percent = 65
             WHERE id = ?",
        )
        .bind(sid)
        .execute(&pool)
        .await
        .unwrap();

        let removed = svc.clear_session(ClearSessionCmd { session_id: sid }).await.unwrap();
        assert_eq!(removed, 4, "种子数据 4 条消息全部硬删");

        // 消息物理消失（软删行也不存在）
        let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM cyan_message WHERE session_id = ?")
            .bind(sid)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(total.0, 0);

        // 统计归零
        let bo = svc.get_session(GetSessionQuery { session_id: sid }).await.unwrap();
        assert!(bo.messages.is_empty());
        assert_eq!(bo.input_tokens, 0);
        assert_eq!(bo.output_tokens, 0);
        assert_eq!(bo.ctx_percent, 0);

        // 幂等：空会话再清返回 0
        let removed = svc.clear_session(ClearSessionCmd { session_id: sid }).await.unwrap();
        assert_eq!(removed, 0);

        // 不存在的会话 → not_found
        let err = svc
            .clear_session(ClearSessionCmd { session_id: 9999 })
            .await
            .unwrap_err();
        assert_eq!(err.code, 2002);
    }
}
