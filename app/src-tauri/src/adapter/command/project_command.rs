//! 项目相关命令：list_projects / open_project / create_project。

use std::sync::Arc;

use tauri::State;

use crate::adapter::dto::{CreateProjectRequest, OpenProjectRequest, ProjectDTO};
use crate::application::project_service::ProjectService;
use crate::error::ServiceError;

/// 最近项目列表
#[tauri::command]
pub async fn list_projects(
    svc: State<'_, Arc<dyn ProjectService>>,
) -> Result<Vec<ProjectDTO>, ServiceError> {
    let bos = svc.list_projects().await?;
    Ok(bos.into_iter().map(ProjectDTO::from).collect())
}

/// 指定文件夹为项目
#[tauri::command]
pub async fn open_project(
    svc: State<'_, Arc<dyn ProjectService>>,
    request: OpenProjectRequest,
) -> Result<ProjectDTO, ServiceError> {
    let bo = svc.open_project(request.into()).await?;
    Ok(ProjectDTO::from(bo))
}

/// 新建项目（脚手架）
#[tauri::command]
pub async fn create_project(
    svc: State<'_, Arc<dyn ProjectService>>,
    request: CreateProjectRequest,
) -> Result<ProjectDTO, ServiceError> {
    let bo = svc.create_project(request.into()).await?;
    Ok(ProjectDTO::from(bo))
}
