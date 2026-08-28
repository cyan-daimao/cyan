//! SessionService 实现：会话与消息编排（Repository 以 Arc<dyn> 注入）。

use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::project::ProjectRepository;
use crate::domain::session::{
    Message, MessageKind, MessageRepository, Session, SessionRepository,
};
use crate::error::ServiceError;
use crate::infra::db::now_local;

use super::{
    AppendMessageCmd, CreateSessionCmd, DeleteSessionCmd, GetSessionQuery, ListSessionQuery,
    MessageBO, ProjectTokenUsageBO, ProjectTokenUsageQuery, SessionBO, SessionSummaryBO,
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
}

/// 会话服务实现
pub struct SessionServiceImpl {
    session_repo: Arc<dyn SessionRepository>,
    message_repo: Arc<dyn MessageRepository>,
    project_repo: Arc<dyn ProjectRepository>,
}

impl SessionServiceImpl {
    /// 构造
    pub fn new(
        session_repo: Arc<dyn SessionRepository>,
        message_repo: Arc<dyn MessageRepository>,
        project_repo: Arc<dyn ProjectRepository>,
    ) -> Self {
        Self {
            session_repo,
            message_repo,
            project_repo,
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
}
