import { create } from 'zustand';
import type { SaveSkillRequest, SkillDTO, SkillScope } from '../types';
import * as skillApi from '../services/skill';
import { errText, toast } from '../utils/feedback';

interface SkillState {
  skills: SkillDTO[];
  loading: boolean;
  /** 已加载的项目维度缓存键（'' = 仅全局），避免输入区反复拉取 */
  loadedFor: string | null;

  /** 加载技能列表；projectPath 传空串只列全局。已缓存同键时跳过（force 强制刷新） */
  load: (projectPath: string, force?: boolean) => Promise<void>;
  /** 新增/编辑保存（全量），成功后刷新 */
  save: (req: SaveSkillRequest) => Promise<boolean>;
  /** 删除，成功后刷新 */
  remove: (scope: SkillScope, fileName: string, projectPath?: string) => Promise<boolean>;
  /** 启用 Switch：改写 enabled 全量保存（PLUGIN_DESIGN 2.5） */
  toggle: (skill: SkillDTO, projectPath: string) => Promise<boolean>;
}

export const useSkillStore = create<SkillState>((set, get) => ({
  skills: [],
  loading: false,
  loadedFor: null,

  load: async (projectPath, force) => {
    if (!force && get().loadedFor === projectPath) return;
    set({ loading: true });
    try {
      const skills = await skillApi.listSkills(projectPath);
      set({ skills, loadedFor: projectPath });
    } catch (e) {
      toast.error(`加载技能列表失败：${errText(e)}`);
    } finally {
      set({ loading: false });
    }
  },

  save: async (req) => {
    try {
      await skillApi.saveSkill(req);
      // 全局/项目技能在同一份列表里，统一按当前列表键强制刷新
      await get().load(get().loadedFor ?? '', true);
      return true;
    } catch (e) {
      toast.error(errText(e));
      return false;
    }
  },

  remove: async (scope, fileName, projectPath) => {
    try {
      await skillApi.deleteSkill(scope, fileName, projectPath);
      await get().load(get().loadedFor ?? '', true);
      return true;
    } catch (e) {
      toast.error(errText(e));
      return false;
    }
  },

  toggle: async (skill, projectPath) => {
    // 插件技能随插件生命周期管理，不单独启停
    if (skill.source === 'plugin') return false;
    return get().save({
      scope: skill.source,
      fileName: skill.id,
      name: skill.name,
      description: skill.description,
      enabled: !skill.enabled,
      content: skill.content,
      projectPath: skill.source === 'project' ? projectPath : undefined,
    });
  },
}));
