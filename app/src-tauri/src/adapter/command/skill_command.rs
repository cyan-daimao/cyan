//! 技能相关命令：list_skills / save_skill / delete_skill。

use std::sync::Arc;

use tauri::State;

use crate::adapter::dto::{
    DeleteSkillRequest, InstallSkillFromGithubRequest, ListSkillRequest, MarketItemDTO,
    SaveSkillRequest, SearchSkillMarketRequest, SkillDTO,
};
use crate::application::skill_service::SkillService;
use crate::error::ServiceError;

/// 技能列表（全局 + 项目合并，同名项目级覆盖全局）
#[tauri::command]
pub async fn list_skills(
    svc: State<'_, Arc<dyn SkillService>>,
    request: ListSkillRequest,
) -> Result<Vec<SkillDTO>, ServiceError> {
    let bos = svc.list_skills(request.into()).await?;
    Ok(bos.into_iter().map(SkillDTO::from).collect())
}

/// 保存技能（按 scope 写盘）
#[tauri::command]
pub async fn save_skill(
    svc: State<'_, Arc<dyn SkillService>>,
    request: SaveSkillRequest,
) -> Result<SkillDTO, ServiceError> {
    let bo = svc.save_skill(request.into()).await?;
    Ok(SkillDTO::from(bo))
}

/// 删除技能（按 scope 删文件，幂等）
#[tauri::command]
pub async fn delete_skill(
    svc: State<'_, Arc<dyn SkillService>>,
    request: DeleteSkillRequest,
) -> Result<(), ServiceError> {
    svc.delete_skill(request.into()).await
}

/// 技能市场搜索（GitHub topic:cyan-skill）
#[tauri::command]
pub async fn search_skill_market(
    svc: State<'_, Arc<dyn SkillService>>,
    request: SearchSkillMarketRequest,
) -> Result<Vec<MarketItemDTO>, ServiceError> {
    let bos = svc.search_skill_market(request.into()).await?;
    Ok(bos.into_iter().map(MarketItemDTO::from).collect())
}

/// 从 GitHub 仓库一键安装技能到全局目录
#[tauri::command]
pub async fn install_skill_from_github(
    svc: State<'_, Arc<dyn SkillService>>,
    request: InstallSkillFromGithubRequest,
) -> Result<Vec<SkillDTO>, ServiceError> {
    let bos = svc.install_skill_from_github(request.into()).await?;
    Ok(bos.into_iter().map(SkillDTO::from).collect())
}
