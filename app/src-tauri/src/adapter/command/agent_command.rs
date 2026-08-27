//! Agent 相关命令：send_task / interrupt_run / approve / rollback_change。

use std::sync::Arc;

use tauri::State;

use crate::adapter::dto::{ApproveRequest, InterruptRequest, RollbackRequest, SendTaskRequest};
use crate::application::agent_service::AgentService;
use crate::error::ServiceError;

/// 发起 Agent 任务（结果走 `agent:event` 事件）
#[tauri::command]
pub async fn send_task(
    svc: State<'_, Arc<dyn AgentService>>,
    request: SendTaskRequest,
) -> Result<(), ServiceError> {
    svc.start_run(request.into()).await
}

/// 中断当前运行（幂等）
#[tauri::command]
pub async fn interrupt_run(
    svc: State<'_, Arc<dyn AgentService>>,
    request: InterruptRequest,
) -> Result<(), ServiceError> {
    svc.interrupt(request.into()).await
}

/// 审批（once/always/reject，幂等）
#[tauri::command]
pub async fn approve(
    svc: State<'_, Arc<dyn AgentService>>,
    request: ApproveRequest,
) -> Result<(), ServiceError> {
    svc.approve(request.into()).await
}

/// checkpoint 回滚（幂等）
#[tauri::command]
pub async fn rollback_change(
    svc: State<'_, Arc<dyn AgentService>>,
    request: RollbackRequest,
) -> Result<(), ServiceError> {
    svc.rollback_change(request.into()).await
}
