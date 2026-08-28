import { call } from './invoke';
import type { MarketItemDTO, SaveSkillRequest, SkillDTO, SkillScope } from '../types';

/** 技能相关命令（PLUGIN_DESIGN 2.4，serde camelCase） */

/** projectPath 传空串时只列全局技能 */
export const listSkills = (projectPath: string) =>
  call<SkillDTO[]>('list_skills', { request: { projectPath } });

export const saveSkill = (request: SaveSkillRequest) =>
  call<SkillDTO>('save_skill', { request });

export const deleteSkill = (scope: SkillScope, fileName: string, projectPath?: string) =>
  call<void>('delete_skill', { request: { scope, fileName, projectPath } });

/** 技能市场：搜索 GitHub 上的技能仓库（keyword 空串返回推荐列表） */
export const searchSkillMarket = (keyword: string) =>
  call<MarketItemDTO[]>('search_skill_market', { request: { keyword } });

/** 从 GitHub 仓库一键安装技能，返回该仓库安装的技能列表 */
export const installSkillFromGithub = (fullName: string) =>
  call<SkillDTO[]>('install_skill_from_github', { request: { fullName } });
