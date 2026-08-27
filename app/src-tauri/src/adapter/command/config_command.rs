//! 配置相关命令：模型 / MCP / 权限规则 CRUD。
//! 变更后额外推送 `config:changed` 事件（TECH_DESIGN 第 7 章）。

use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};

use crate::adapter::dto::{
    DeleteMcpRequest, DeleteModelRequest, DeletePermRuleRequest, McpServerDTO, ModelDTO,
    PermRuleDTO, SaveMcpRequest, SaveModelRequest, SavePermRuleRequest, SetDefaultModelRequest,
    ToggleMcpRequest,
};
use crate::application::config_service::ConfigService;
use crate::error::ServiceError;

/// `config:changed` 事件通道
const CONFIG_CHANGED_CHANNEL: &str = "config:changed";

/// 推送配置变更事件（kind: model/mcp/perm_rule）
fn emit_config_changed(app: &AppHandle, kind: &str) {
    if let Err(e) = app.emit(CONFIG_CHANGED_CHANNEL, serde_json::json!({ "kind": kind })) {
        tracing::warn!(error = %e, "config:changed 事件推送失败");
    }
}

/// 模型列表
#[tauri::command]
pub async fn list_models(
    svc: State<'_, Arc<dyn ConfigService>>,
) -> Result<Vec<ModelDTO>, ServiceError> {
    let bos = svc.list_models().await?;
    Ok(bos.into_iter().map(ModelDTO::from).collect())
}

/// 保存模型（按 name 幂等 upsert）
#[tauri::command]
pub async fn save_model(
    app: AppHandle,
    svc: State<'_, Arc<dyn ConfigService>>,
    request: SaveModelRequest,
) -> Result<ModelDTO, ServiceError> {
    let bo = svc.save_model(request.into()).await?;
    emit_config_changed(&app, "model");
    Ok(ModelDTO::from(bo))
}

/// 删除模型（默认保护）
#[tauri::command]
pub async fn delete_model(
    app: AppHandle,
    svc: State<'_, Arc<dyn ConfigService>>,
    request: DeleteModelRequest,
) -> Result<(), ServiceError> {
    svc.delete_model(request.into()).await?;
    emit_config_changed(&app, "model");
    Ok(())
}

/// 设为默认模型
#[tauri::command]
pub async fn set_default_model(
    app: AppHandle,
    svc: State<'_, Arc<dyn ConfigService>>,
    request: SetDefaultModelRequest,
) -> Result<(), ServiceError> {
    svc.set_default_model(request.into()).await?;
    emit_config_changed(&app, "model");
    Ok(())
}

/// MCP 服务器列表
#[tauri::command]
pub async fn list_mcp_servers(
    svc: State<'_, Arc<dyn ConfigService>>,
) -> Result<Vec<McpServerDTO>, ServiceError> {
    let bos = svc.list_mcp_servers().await?;
    Ok(bos.into_iter().map(McpServerDTO::from).collect())
}

/// 保存 MCP 服务器（按 name 幂等 upsert）
#[tauri::command]
pub async fn save_mcp_server(
    app: AppHandle,
    svc: State<'_, Arc<dyn ConfigService>>,
    request: SaveMcpRequest,
) -> Result<McpServerDTO, ServiceError> {
    let bo = svc.save_mcp_server(request.into()).await?;
    emit_config_changed(&app, "mcp");
    Ok(McpServerDTO::from(bo))
}

/// 启停 MCP 服务器（幂等）
#[tauri::command]
pub async fn toggle_mcp_server(
    app: AppHandle,
    svc: State<'_, Arc<dyn ConfigService>>,
    request: ToggleMcpRequest,
) -> Result<McpServerDTO, ServiceError> {
    let bo = svc.toggle_mcp_server(request.into()).await?;
    emit_config_changed(&app, "mcp");
    Ok(McpServerDTO::from(bo))
}

/// 删除 MCP 服务器（幂等）
#[tauri::command]
pub async fn delete_mcp_server(
    app: AppHandle,
    svc: State<'_, Arc<dyn ConfigService>>,
    request: DeleteMcpRequest,
) -> Result<(), ServiceError> {
    svc.delete_mcp_server(request.into()).await?;
    emit_config_changed(&app, "mcp");
    Ok(())
}

/// 权限规则列表（sort 升序）
#[tauri::command]
pub async fn list_perm_rules(
    svc: State<'_, Arc<dyn ConfigService>>,
) -> Result<Vec<PermRuleDTO>, ServiceError> {
    let bos = svc.list_perm_rules().await?;
    Ok(bos.into_iter().map(PermRuleDTO::from).collect())
}

/// 保存权限规则（按 tool+pattern 幂等 upsert）
#[tauri::command]
pub async fn save_perm_rule(
    app: AppHandle,
    svc: State<'_, Arc<dyn ConfigService>>,
    request: SavePermRuleRequest,
) -> Result<PermRuleDTO, ServiceError> {
    let bo = svc.save_perm_rule(request.into()).await?;
    emit_config_changed(&app, "perm_rule");
    Ok(PermRuleDTO::from(bo))
}

/// 删除权限规则（幂等）
#[tauri::command]
pub async fn delete_perm_rule(
    app: AppHandle,
    svc: State<'_, Arc<dyn ConfigService>>,
    request: DeletePermRuleRequest,
) -> Result<(), ServiceError> {
    svc.delete_perm_rule(request.into()).await?;
    emit_config_changed(&app, "perm_rule");
    Ok(())
}
