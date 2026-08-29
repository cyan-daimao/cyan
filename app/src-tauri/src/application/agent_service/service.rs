//! AgentService 实现：任务编排、审批流转、中断与回滚。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::domain::agent::{
    AgentRun, ApprovalDecision, CheckpointGateway, CheckpointRepository, LlmGateway, PermMode,
    RunEventSink, ToolExecutor,
};
use crate::domain::config::{ModelRepository, PermRuleRepository, PermissionRule, RuleScope};
use crate::domain::project::ProjectRepository;
use crate::domain::session::{Message, MessageKind, MessageRepository, SessionRepository};
use crate::domain::shared::ProjectPath;
use crate::error::ServiceError;
use crate::infra::db::now_local;
use crate::infra::secret;

use super::runner::{run_loop, RunContext};
use super::{ApproveCmd, InterruptCmd, RollbackCmd, StartRunCmd};

/// Agent 服务
#[async_trait]
pub trait AgentService: Send + Sync {
    /// 发起 Agent 任务（结果走 `agent:event` 事件）
    async fn start_run(&self, cmd: StartRunCmd) -> Result<(), ServiceError>;
    /// 中断当前运行（幂等）
    async fn interrupt(&self, cmd: InterruptCmd) -> Result<(), ServiceError>;
    /// 审批（幂等：重复审批返回已决断）
    async fn approve(&self, cmd: ApproveCmd) -> Result<(), ServiceError>;
    /// checkpoint 回滚（幂等）
    async fn rollback_change(&self, cmd: RollbackCmd) -> Result<(), ServiceError>;
}

/// Agent 服务实现
pub struct AgentServiceImpl {
    ctx: RunContext,
    session_repo: Arc<dyn SessionRepository>,
    message_repo: Arc<dyn MessageRepository>,
    project_repo: Arc<dyn ProjectRepository>,
    model_repo: Arc<dyn ModelRepository>,
    checkpoint_repo: Arc<dyn CheckpointRepository>,
    checkpoint_gateway: Arc<dyn CheckpointGateway>,
    /// 运行表（session_id → AgentRun，内存态不持久化）
    runs: Arc<Mutex<HashMap<i64, Arc<AgentRun>>>>,
}

impl AgentServiceImpl {
    /// 构造
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        session_repo: Arc<dyn SessionRepository>,
        message_repo: Arc<dyn MessageRepository>,
        project_repo: Arc<dyn ProjectRepository>,
        checkpoint_repo: Arc<dyn CheckpointRepository>,
        perm_repo: Arc<dyn PermRuleRepository>,
        model_repo: Arc<dyn ModelRepository>,
        llm: Arc<dyn LlmGateway>,
        executor: Arc<dyn ToolExecutor>,
        checkpoint_gateway: Arc<dyn CheckpointGateway>,
        sink: Arc<dyn RunEventSink>,
    ) -> Self {
        let ctx = RunContext {
            session_repo: session_repo.clone(),
            message_repo: message_repo.clone(),
            checkpoint_repo: checkpoint_repo.clone(),
            perm_repo,
            llm,
            executor,
            sink,
        };
        Self {
            ctx,
            session_repo,
            message_repo,
            project_repo,
            model_repo,
            checkpoint_repo,
            checkpoint_gateway,
            runs: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 查找会话的活跃运行
    fn active_run(&self, session_id: i64) -> Option<Arc<AgentRun>> {
        self.runs
            .lock()
            .expect("runs 锁中毒")
            .get(&session_id)
            .cloned()
    }
}

/// 用户消息准备（start_run 的前置步骤）：
/// skip_append=false → 首条任务生成标题 + 追加用户消息落库；
/// skip_append=true → 校验会话最后一条未删消息为 user kind（编辑重发场景），不新增消息
async fn prepare_user_message(
    message_repo: &Arc<dyn MessageRepository>,
    session_repo: &Arc<dyn SessionRepository>,
    session: &mut crate::domain::session::Session,
    text: &str,
    skip_append: bool,
) -> Result<(), ServiceError> {
    session.messages = message_repo.list_by_session(session.id).await?;
    if skip_append {
        match session.messages.last() {
            Some(m) if m.kind == MessageKind::User => return Ok(()),
            _ => {
                return Err(ServiceError::validation(
                    "最后一条不是用户消息，无法重新生成",
                ))
            }
        }
    }
    session.apply_first_task_title(text);
    let mut user_msg = session
        .append_message(Message::new(
            session.id,
            MessageKind::User,
            Message::text_payload(text.trim()),
            now_local(),
        ))
        .clone();
    message_repo.insert(&mut user_msg).await?;
    session_repo.update(session).await?;
    Ok(())
}

#[async_trait]
impl AgentService for AgentServiceImpl {
    async fn start_run(&self, cmd: StartRunCmd) -> Result<(), ServiceError> {
        if cmd.text.trim().is_empty() {
            return Err(ServiceError::validation("任务文本不能为空"));
        }
        // 校验会话存在 + idle
        let mut session = self
            .session_repo
            .find_by_id(cmd.session_id)
            .await?
            .ok_or_else(|| ServiceError::not_found(format!("会话不存在：{}", cmd.session_id)))?;
        if let Some(existing) = self.active_run(cmd.session_id) {
            if existing.state() != crate::domain::agent::RunState::Idle {
                return Err(ServiceError::conflict("当前会话已有运行中的任务"));
            }
        }
        // 校验项目存在
        let project = self
            .project_repo
            .find_by_id(session.project_id)
            .await?
            .ok_or_else(|| ServiceError::not_found("会话所属项目不存在"))?;
        let root = ProjectPath::new(&project.path)?;
        // 解析模型与 API Key
        let model = if cmd.model.trim().is_empty() {
            self.model_repo.find_default().await?
        } else {
            self.model_repo.find_by_name(cmd.model.trim()).await?
        }
        .ok_or_else(|| ServiceError::not_found("模型配置不存在，请先在设置中配置模型"))?;
        let api_key = secret::load_api_key(&model.name)
            .map_err(|_| ServiceError::validation(format!("模型 {} 未配置 API Key", model.name)))?;
        let mode = PermMode::parse(&cmd.perm_mode);
        // 三级作用域规则：全局 + 本项目 + 本会话
        let rules = self
            .ctx
            .perm_repo
            .list_visible(cmd.session_id, session.project_id)
            .await?;

        // 首条任务生成标题 + 用户消息落库（skip_append：编辑重发场景，校验末尾为 user 不新增）
        prepare_user_message(
            &self.message_repo,
            &self.session_repo,
            &mut session,
            &cmd.text,
            cmd.skip_append,
        )
        .await?;

        // 启动运行
        let run = Arc::new(AgentRun::new(session.id));
        run.start()?;
        self.runs
            .lock()
            .expect("runs 锁中毒")
            .insert(session.id, run.clone());
        let runs = self.runs.clone();
        let ctx = self.ctx.clone();
        let run_clone = run.clone();
        let session_id = session.id;
        let disabled = cmd.disabled_tools.clone();
        tokio::spawn(async move {
            run_loop(ctx, run_clone, session, root, model, api_key, rules, mode, disabled).await;
            runs.lock().expect("runs 锁中毒").remove(&session_id);
        });
        Ok(())
    }

    async fn interrupt(&self, cmd: InterruptCmd) -> Result<(), ServiceError> {
        // 幂等：无活跃运行直接返回
        if let Some(run) = self.active_run(cmd.session_id) {
            run.interrupt();
        }
        Ok(())
    }

    async fn approve(&self, cmd: ApproveCmd) -> Result<(), ServiceError> {
        let decision = ApprovalDecision::parse(&cmd.decision)
            .ok_or_else(|| ServiceError::validation(format!("非法审批决断：{}", cmd.decision)))?;
        let Some(run) = self.active_run(cmd.session_id) else {
            return Err(ServiceError::not_found("当前会话没有运行中的任务"));
        };
        // 幂等：callId 不存在（已决断）时返回 None，视为已决断
        let Some((tool, arg)) = run.approve(&cmd.call_id, decision) else {
            return Ok(());
        };
        // 「总是允许」自动推导规则落库（按用户选择的作用域 upsert，缺省本会话）
        if decision == ApprovalDecision::Always {
            let scope = cmd
                .always_scope
                .as_deref()
                .and_then(RuleScope::parse)
                .unwrap_or(RuleScope::Session);
            let mut rule = PermissionRule::always_allow_from(&tool, &arg);
            match scope {
                RuleScope::Global => {}
                RuleScope::Project | RuleScope::Session => {
                    let session = self
                        .session_repo
                        .find_by_id(cmd.session_id)
                        .await?
                        .ok_or_else(|| ServiceError::not_found("会话不存在"))?;
                    rule.project_id = Some(session.project_id);
                    if scope == RuleScope::Session {
                        rule.session_id = Some(cmd.session_id);
                    }
                }
            }
            match self
                .ctx
                .perm_repo
                .find_by_tool_pattern(&rule.tool, &rule.pattern, rule.project_id, rule.session_id)
                .await?
            {
                Some(_) => {}
                None => self.ctx.perm_repo.insert(&mut rule).await?,
            }
        }
        Ok(())
    }

    async fn rollback_change(&self, cmd: RollbackCmd) -> Result<(), ServiceError> {
        let checkpoint = self
            .checkpoint_repo
            .find_by_id(cmd.change_id)
            .await?
            .ok_or_else(|| ServiceError::not_found(format!("变更不存在：{}", cmd.change_id)))?;
        // 幂等：已回滚直接返回
        if checkpoint.rolled_back {
            return Ok(());
        }
        let session = self
            .session_repo
            .find_by_id(checkpoint.session_id)
            .await?
            .ok_or_else(|| ServiceError::not_found("变更所属会话不存在"))?;
        let project = self
            .project_repo
            .find_by_id(session.project_id)
            .await?
            .ok_or_else(|| ServiceError::not_found("变更所属项目不存在"))?;
        let root = ProjectPath::new(&project.path)?;
        self.checkpoint_gateway
            .rollback(root.root(), &checkpoint.git_ref, &checkpoint.file_path)?;
        self.checkpoint_repo.mark_rolled_back(cmd.change_id).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::session::{MessageRepository, SessionRepository};
    use crate::infra::db::session_repo::{MessageRepositoryImpl, SessionRepositoryImpl};
    use sqlx::SqlitePool;

    async fn seed(pool: &SqlitePool) -> (i64, i64) {
        let pid = sqlx::query(
            "INSERT INTO cyan_project (name, path, created_by, updated_by, created_at, updated_at)
             VALUES ('demo', '/tmp/demo', 'local', 'local', '2026-08-27 10:00:00', '2026-08-27 10:00:00')",
        )
        .execute(pool)
        .await
        .unwrap()
        .last_insert_rowid();
        (pid, 0)
    }

    fn repos(
        pool: &SqlitePool,
    ) -> (Arc<dyn MessageRepository>, Arc<dyn SessionRepository>) {
        (
            Arc::new(MessageRepositoryImpl::new(pool.clone())),
            Arc::new(SessionRepositoryImpl::new(pool.clone())),
        )
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn normal_path_appends_user_message(pool: SqlitePool) {
        let (pid, _) = seed(&pool).await;
        let (msg_repo, session_repo) = repos(&pool);
        let mut session = crate::domain::session::Session::new(pid, now_local());
        session_repo.insert(&mut session).await.unwrap();

        prepare_user_message(&msg_repo, &session_repo, &mut session, "你好", false)
            .await
            .unwrap();
        let msgs = msg_repo.list_by_session(session.id).await.unwrap();
        assert_eq!(msgs.len(), 1, "正常路径应追加用户消息");
        assert_eq!(msgs[0].text().as_deref(), Some("你好"));
        // 标题由首条任务生成
        let saved = session_repo.find_by_id(session.id).await.unwrap().unwrap();
        assert_eq!(saved.title, "你好");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn skip_append_keeps_messages_when_last_is_user(pool: SqlitePool) {
        let (pid, _) = seed(&pool).await;
        let (msg_repo, session_repo) = repos(&pool);
        let mut session = crate::domain::session::Session::new(pid, now_local());
        session_repo.insert(&mut session).await.unwrap();
        prepare_user_message(&msg_repo, &session_repo, &mut session, "原始消息", false)
            .await
            .unwrap();

        // skip_append：不新增消息
        let mut session = session_repo.find_by_id(session.id).await.unwrap().unwrap();
        prepare_user_message(&msg_repo, &session_repo, &mut session, "编辑后的文本", true)
            .await
            .unwrap();
        let msgs = msg_repo.list_by_session(session.id).await.unwrap();
        assert_eq!(msgs.len(), 1, "skip_append 不应新增消息");
        assert_eq!(msgs[0].text().as_deref(), Some("原始消息"));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn skip_append_rejects_when_last_is_not_user(pool: SqlitePool) {
        let (pid, _) = seed(&pool).await;
        let (msg_repo, session_repo) = repos(&pool);
        let mut session = crate::domain::session::Session::new(pid, now_local());
        session_repo.insert(&mut session).await.unwrap();
        // 末尾是 assistant 消息
        prepare_user_message(&msg_repo, &session_repo, &mut session, "问", false)
            .await
            .unwrap();
        let mut m = Message::new(session.id, MessageKind::Assistant, Message::text_payload("答"), now_local());
        m.seq = 2;
        msg_repo.insert(&mut m).await.unwrap();

        let mut session = session_repo.find_by_id(session.id).await.unwrap().unwrap();
        let err = prepare_user_message(&msg_repo, &session_repo, &mut session, "重发", true)
            .await
            .unwrap_err();
        assert_eq!(err.code, 1001);
        assert!(err.message.contains("最后一条不是用户消息"));

        // 空会话同样拒绝
        let mut s2 = crate::domain::session::Session::new(pid, now_local());
        session_repo.insert(&mut s2).await.unwrap();
        assert!(prepare_user_message(&msg_repo, &session_repo, &mut s2, "重发", true)
            .await
            .is_err());
    }
}
