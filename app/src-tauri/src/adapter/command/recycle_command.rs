//! 回收站命令：list_recycle_bin / restore_recycle_item。

use std::sync::Arc;

use tauri::State;

use crate::adapter::dto::{RecycleBinDTO, RestoreRecycleItemRequest};
use crate::application::recycle_service::RecycleService;
use crate::error::ServiceError;

/// 回收站全量列表（六类软删记录）
#[tauri::command]
pub async fn list_recycle_bin(
    svc: State<'_, Arc<dyn RecycleService>>,
) -> Result<RecycleBinDTO, ServiceError> {
    let bo = svc.list_recycle_bin().await?;
    Ok(RecycleBinDTO::from(bo))
}

/// 恢复回收站对象（幂等）
#[tauri::command]
pub async fn restore_recycle_item(
    svc: State<'_, Arc<dyn RecycleService>>,
    request: RestoreRecycleItemRequest,
) -> Result<(), ServiceError> {
    svc.restore_item(request.into()).await
}
