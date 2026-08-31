import { call } from './invoke';
import type { MarketItemDTO, MarketSource, SaveSkillRequest, SkillDTO, SkillScope } from '../types';

/** 技能相关命令（PLUGIN_DESIGN 2.4，serde camelCase） */

/** projectPath 传空串时只列全局技能 */
export const listSkills = (projectPath: string) =>
  call<SkillDTO[]>('list_skills', { request: { projectPath } });

export const saveSkill = (request: SaveSkillRequest) =>
  call<SkillDTO>('save_skill', { request });

export const deleteSkill = (scope: SkillScope, fileName: string, projectPath?: string) =>
  call<void>('delete_skill', { request: { scope, fileName, projectPath } });

/** 技能市场搜索（GitHub topic:cyan-skill 搜索 / Gitee owner-repo 直达；keyword 空串时 GitHub 返回推荐列表） */
export const searchSkillMarket = (keyword: string, source: MarketSource = 'github') =>
  call<MarketItemDTO[]>('search_skill_market', { request: { keyword, source } });

/** 从远端仓库一键安装技能（source=gitee 走 Gitee 归档下载，缺省 GitHub），返回该仓库安装的技能列表 */
export const installSkillFromGithub = (fullName: string, source: MarketSource = 'github') =>
  call<SkillDTO[]>('install_skill_from_github', { request: { fullName, source } });
