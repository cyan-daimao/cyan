//! 会话相关命令：list_sessions / get_session / create_session / delete_session / project_token_usage
//! + 回收站三命令（list_deleted_sessions / restore_session / purge_recycle_bin）。

use std::sync::Arc;

use tauri::State;

use crate::adapter::dto::{
    CreateSessionRequest, DeleteSessionRequest, EditMessageRequest, GetSessionRequest,
    ListSessionRequest, ProjectTokenUsageDTO, ProjectTokenUsageRequest, RestoreSessionRequest,
    SessionDTO, SessionSummaryDTO,
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

/// 回收站：软删会话列表（带所属项目名称/路径）
#[tauri::command]
pub async fn list_deleted_sessions(
    svc: State<'_, Arc<dyn SessionService>>,
) -> Result<Vec<SessionDTO>, ServiceError> {
    let bos = svc.list_deleted_sessions().await?;
    Ok(bos.into_iter().map(SessionDTO::from).collect())
}

/// 恢复会话（含全部软删消息，幂等）
#[tauri::command]
pub async fn restore_session(
    svc: State<'_, Arc<dyn SessionService>>,
    request: RestoreSessionRequest,
) -> Result<(), ServiceError> {
    svc.restore_session(request.into()).await
}

/// 清空回收站：全库软删记录硬删，返回总删除行数
#[tauri::command]
pub async fn purge_recycle_bin(
    svc: State<'_, Arc<dyn SessionService>>,
) -> Result<i64, ServiceError> {
    svc.purge_recycle_bin().await
}

/// 编辑消息（编辑即截断重发），返回更新+截断后的完整会话
#[tauri::command]
pub async fn edit_message(
    svc: State<'_, Arc<dyn SessionService>>,
    request: EditMessageRequest,
) -> Result<SessionDTO, ServiceError> {
    let bo = svc.edit_message(request.into()).await?;
    Ok(SessionDTO::from(bo))
}
