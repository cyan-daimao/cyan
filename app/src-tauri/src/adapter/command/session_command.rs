//! 会话相关命令：list_sessions / get_session / create_session / delete_session。

use std::sync::Arc;

use tauri::State;

use crate::adapter::dto::{
    CreateSessionRequest, DeleteSessionRequest, GetSessionRequest, ListSessionRequest,
    ProjectTokenUsageDTO, ProjectTokenUsageRequest, SessionDTO, SessionSummaryDTO,
};
use crate::application::session_service::SessionService;
use crate::error::ServiceError;

/// 会话列表/搜索
#[tauri::command]
pub async fn list_sessions(
    svc: State<'_, Arc<dyn SessionService>>,
    request: ListSessionRequest,
) -> Result<Vec<SessionSummaryDTO>, ServiceError> {
    let bos = svc.list_sessions(request.into()).await?;
    Ok(bos.into_iter().map(SessionSummaryDTO::from).collect())
}

/// 打开会话（含全部消息）
#[tauri::command]
pub async fn get_session(
    svc: State<'_, Arc<dyn SessionService>>,
    request: GetSessionRequest,
) -> Result<SessionDTO, ServiceError> {
    let bo = svc.get_session(request.into()).await?;
    Ok(SessionDTO::from(bo))
}

/// 新建会话
#[tauri::command]
pub async fn create_session(
    svc: State<'_, Arc<dyn SessionService>>,
    request: CreateSessionRequest,
) -> Result<SessionDTO, ServiceError> {
    let bo = svc.create_session(request.into()).await?;
    Ok(SessionDTO::from(bo))
}

/// 删除会话（软删，幂等）
#[tauri::command]
pub async fn delete_session(
    svc: State<'_, Arc<dyn SessionService>>,
    request: DeleteSessionRequest,
) -> Result<(), ServiceError> {
    svc.delete_session(request.into()).await
}

/// 项目级 token 用量聚合
#[tauri::command]
pub async fn project_token_usage(
    svc: State<'_, Arc<dyn SessionService>>,
    request: ProjectTokenUsageRequest,
) -> Result<ProjectTokenUsageDTO, ServiceError> {
    let bo = svc.token_usage(request.into()).await?;
    Ok(ProjectTokenUsageDTO::from(bo))
}
