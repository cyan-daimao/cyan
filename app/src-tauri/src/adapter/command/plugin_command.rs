//! 插件相关命令：list_plugins / install_plugin / toggle_plugin / delete_plugin。

use std::sync::Arc;

use tauri::State;

use crate::adapter::dto::{
    DeletePluginRequest, InstallPluginFromGithubRequest, InstallPluginRequest, MarketItemDTO,
    PluginDTO, SearchMarketplaceRequest, TogglePluginRequest,
};
use crate::application::plugin_service::PluginService;
use crate::error::ServiceError;

/// 插件列表
#[tauri::command]
pub async fn list_plugins(
    svc: State<'_, Arc<dyn PluginService>>,
) -> Result<Vec<PluginDTO>, ServiceError> {
    let bos = svc.list_plugins().await?;
    Ok(bos.into_iter().map(PluginDTO::from).collect())
}

/// 安装插件（zip 或目录）
#[tauri::command]
pub async fn install_plugin(
    svc: State<'_, Arc<dyn PluginService>>,
    request: InstallPluginRequest,
) -> Result<PluginDTO, ServiceError> {
    let bo = svc.install_plugin(request.into()).await?;
    Ok(PluginDTO::from(bo))
}

/// 启停插件（幂等）
#[tauri::command]
pub async fn toggle_plugin(
    svc: State<'_, Arc<dyn PluginService>>,
    request: TogglePluginRequest,
) -> Result<PluginDTO, ServiceError> {
    let bo = svc.toggle_plugin(request.into()).await?;
    Ok(PluginDTO::from(bo))
}

/// 卸载插件（幂等）
#[tauri::command]
pub async fn delete_plugin(
    svc: State<'_, Arc<dyn PluginService>>,
    request: DeletePluginRequest,
) -> Result<(), ServiceError> {
    svc.delete_plugin(request.into()).await
}

/// 插件市场搜索（GitHub topic:cyan-plugin）
#[tauri::command]
pub async fn search_marketplace(
    svc: State<'_, Arc<dyn PluginService>>,
    request: SearchMarketplaceRequest,
) -> Result<Vec<MarketItemDTO>, ServiceError> {
    let bos = svc.search_marketplace(request.into()).await?;
    Ok(bos.into_iter().map(MarketItemDTO::from).collect())
}

/// 从 GitHub 仓库一键安装
#[tauri::command]
pub async fn install_plugin_from_github(
    svc: State<'_, Arc<dyn PluginService>>,
    request: InstallPluginFromGithubRequest,
) -> Result<PluginDTO, ServiceError> {
    let bo = svc.install_plugin_from_github(request.into()).await?;
    Ok(PluginDTO::from(bo))
}
