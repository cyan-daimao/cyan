//! 文件相关命令：file_tree / file_preview。

use std::sync::Arc;

use tauri::State;

use crate::adapter::dto::{FileNodeDTO, FilePreviewDTO, FilePreviewRequest, FileTreeRequest};
use crate::application::project_service::ProjectService;
use crate::error::ServiceError;

/// 文件树
#[tauri::command]
pub async fn file_tree(
    svc: State<'_, Arc<dyn ProjectService>>,
    request: FileTreeRequest,
) -> Result<Vec<FileNodeDTO>, ServiceError> {
    let bos = svc.file_tree(request.into()).await?;
    Ok(bos.into_iter().map(FileNodeDTO::from).collect())
}

/// 文件预览（≤64KB，超出 truncated=true；二进制拒绝）
#[tauri::command]
pub async fn file_preview(
    svc: State<'_, Arc<dyn ProjectService>>,
    request: FilePreviewRequest,
) -> Result<FilePreviewDTO, ServiceError> {
    let bo = svc.file_preview(request.into()).await?;
    Ok(FilePreviewDTO::from(bo))
}
