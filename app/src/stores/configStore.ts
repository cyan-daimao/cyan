import { create } from 'zustand';
import type {
  McpServerDTO,
  ModelDTO,
  PermAction,
  PermMode,
  PermRuleDTO,
  RuleScope,
  SaveModelRequest,
} from '../types';
import * as configApi from '../services/config';
import { setSessionModel as apiSetSessionModel } from '../services/session';
import { errText, toast } from '../utils/feedback';

const PERM_MODE_KEY = 'cyan.permMode';
const ACTIVE_MODEL_KEY = 'cyan.activeModel';
const DISABLED_TOOLS_KEY = 'cyan.disabledTools';
const LANG_KEY = 'cyan.lang';
const THEME_COLOR_KEY = 'cyan.themeColor';
const BG_MODE_KEY = 'cyan.bgMode';

function loadPermMode(): PermMode {
  const v = localStorage.getItem(PERM_MODE_KEY);
  return v === 'plan' || v === 'ask' || v === 'auto' ? v : 'ask';
}

function loadJson<T>(key: string, fallback: T): T {
  try {
    const v = localStorage.getItem(key);
    return v ? (JSON.parse(v) as T) : fallback;
  } catch {
    return fallback;
  }
}

/** 界面语言 */
export type Lang = 'zh' | 'en';

/** 背景主题 */
export type BgMode = 'light' | 'dark';

/** 预设主题色 */
export const THEME_COLORS = [
  { name: '青色（默认）', value: '#00B39E' },
  { name: '蓝色', value: '#3AA8FF' },
  { name: '紫色', value: '#7C5CFF' },
  { name: '绿色', value: '#22C55E' },
  { name: '橙色', value: '#F59E0B' },
] as const;

interface ConfigState {
  models: ModelDTO[];
  mcpServers: McpServerDTO[];
  /** 当前会话可见的权限规则（全局 + 本项目 + 本会话） */
  permRules: PermRuleDTO[];
  /** 全局权限规则（设置页管理） */
  globalRules: PermRuleDTO[];
  /** 权限模式（PRD 2.2，默认 ask） */
  permMode: PermMode;
  /** 输入区当前选中的模型名（send_task 按模型名传参） */
  activeModel: string | null;
  /** 会话级模型偏好（内存态；sessionId → 模型名），优先级高于 activeModel */
  sessionModels: Record<number, string>;
  /** 「能力」面板禁用的内置工具名（随 send_task 下发） */
  disabledTools: string[];
  /** 界面语言（antd locale 级别） */
  lang: Lang;
  /** 主题色（antd colorPrimary + CSS 品牌变量） */
  themeColor: string;
  /** 背景主题（浅色/深色） */
  bgMode: BgMode;
  loadingModels: boolean;
  loadingMcp: boolean;
  loadingPerms: boolean;

  setPermMode: (mode: PermMode) => void;
  setActiveModel: (name: string) => void;
  /**
   * 设置会话级模型偏好：调后端 set_session_model 后写本地；
   * model 为空串则从 map 删除该键（清除偏好，跟随全局）。
   * 由打开会话的路径用 preferredModel 播种时走 seedSessionModel（不调后端）。
   */
  setSessionModel: (sessionId: number, model: string) => Promise<boolean>;
  /** 打开会话时用 SessionDTO.preferredModel 播种本地 map（纯内存，不调后端） */
  seedSessionModel: (sessionId: number, model: string | null | undefined) => void;
  /** 切换内置工具启用状态 */
  setToolEnabled: (name: string, enabled: boolean) => void;
  setLang: (lang: Lang) => void;
  setThemeColor: (color: string) => void;
  setBgMode: (mode: BgMode) => void;
  loadAll: () => Promise<void>;

  /* ---- 模型 ---- */
  loadModels: () => Promise<void>;
  saveModel: (req: SaveModelRequest) => Promise<boolean>;
  deleteModel: (id: number) => Promise<boolean>;
  setDefault: (id: number) => Promise<boolean>;

  /* ---- MCP ---- */
  loadMcpServers: () => Promise<void>;
  saveMcpServer: (
    id: number | undefined,
    name: string,
    command: string,
    transport?: 'stdio' | 'sse',
    headers?: string,
  ) => Promise<boolean>;
  toggleMcp: (id: number, enable: boolean) => Promise<boolean>;
  deleteMcp: (id: number) => Promise<boolean>;

  /* ---- 权限规则（三级作用域） ---- */
  loadVisibleRules: (sessionId: number, projectId: number) => Promise<void>;
  loadGlobalRules: () => Promise<void>;
  savePermRule: (
    id: number | undefined,
    scope: RuleScope | undefined,
    projectId: number | undefined,
    sessionId: number | undefined,
    tool: string,
    pattern: string,
    action: PermAction,
    sort: number,
  ) => Promise<boolean>;
  /** 删除规则（不自动刷新列表，由调用方按上下文刷新） */
  deletePermRule: (id: number) => Promise<boolean>;
}

export const useConfigStore = create<ConfigState>((set, get) => ({
  models: [],
  mcpServers: [],
  permRules: [],
  globalRules: [],
  permMode: loadPermMode(),
  activeModel: localStorage.getItem(ACTIVE_MODEL_KEY),
  sessionModels: {},
  disabledTools: loadJson<string[]>(DISABLED_TOOLS_KEY, []),
  lang: loadJson<Lang>(LANG_KEY, 'zh'),
  themeColor: localStorage.getItem(THEME_COLOR_KEY) ?? THEME_COLORS[0].value,
  bgMode: loadJson<BgMode>(BG_MODE_KEY, 'light'),
  loadingModels: false,
  loadingMcp: false,
  loadingPerms: false,

  setPermMode: (mode) => {
    localStorage.setItem(PERM_MODE_KEY, mode);
    set({ permMode: mode });
  },

  setActiveModel: (name) => {
    localStorage.setItem(ACTIVE_MODEL_KEY, name);
    set({ activeModel: name });
  },

  setSessionModel: async (sessionId, model) => {
    try {
      await apiSetSessionModel(sessionId, model);
      get().seedSessionModel(sessionId, model || null);
      return true;
    } catch (e) {
      toast.error(errText(e));
      return false;
    }
  },

  seedSessionModel: (sessionId, model) => {
    const map = { ...get().sessionModels };
    if (model) map[sessionId] = model;
    else delete map[sessionId];
    set({ sessionModels: map });
  },

  setToolEnabled: (name, enabled) => {
    const cur = get().disabledTools;
    const next = enabled ? cur.filter((t) => t !== name) : [...new Set([...cur, name])];
    localStorage.setItem(DISABLED_TOOLS_KEY, JSON.stringify(next));
    set({ disabledTools: next });
  },

  setLang: (lang) => {
    localStorage.setItem(LANG_KEY, JSON.stringify(lang));
    set({ lang });
  },

  setThemeColor: (color) => {
    localStorage.setItem(THEME_COLOR_KEY, color);
    set({ themeColor: color });
  },

  setBgMode: (mode) => {
    localStorage.setItem(BG_MODE_KEY, JSON.stringify(mode));
    set({ bgMode: mode });
  },

  loadAll: async () => {
    // 权限规则为对话级，不在此处加载（由会话入口按 sessionId 加载）
    await Promise.all([get().loadModels(), get().loadMcpServers()]);
  },

  /* ---- 模型 ---- */

  loadModels: async () => {
    set({ loadingModels: true });
    try {
      const models = await configApi.listModels();
      set({ models });
      // 校准选择器：未选或所选已停用/删除时回落到默认模型
      const { activeModel, setActiveModel } = get();
      const enabled = models.filter((m) => m.status === 'enabled');
      if (!enabled.some((m) => m.name === activeModel)) {
        const fallback = models.find((m) => m.isDefault && m.status === 'enabled') ?? enabled[0];
        if (fallback) setActiveModel(fallback.name);
        else set({ activeModel: null });
      }
    } catch (e) {
      toast.error(`加载模型配置失败：${errText(e)}`);
    } finally {
      set({ loadingModels: false });
    }
  },

  saveModel: async (req) => {
    try {
      await configApi.saveModel(req);
      await get().loadModels();
      return true;
    } catch (e) {
      toast.error(errText(e));
      return false;
    }
  },

  deleteModel: async (id) => {
    try {
      await configApi.deleteModel(id);
      await get().loadModels();
      return true;
    } catch (e) {
      toast.error(errText(e));
      return false;
    }
  },

  setDefault: async (id) => {
    try {
      await configApi.setDefaultModel(id);
      await get().loadModels();
      // 同步输入区模型选择器
      const m = get().models.find((x) => x.id === id);
      if (m) get().setActiveModel(m.name);
      return true;
    } catch (e) {
      toast.error(errText(e));
      return false;
    }
  },

  /* ---- MCP ---- */

  loadMcpServers: async () => {
    set({ loadingMcp: true });
    try {
      set({ mcpServers: await configApi.listMcpServers() });
    } catch (e) {
      toast.error(`加载 MCP 服务器失败：${errText(e)}`);
    } finally {
      set({ loadingMcp: false });
    }
  },

  saveMcpServer: async (id, name, command, transport = 'stdio', headers = '{}') => {
    try {
      await configApi.saveMcpServer(id, name, command, transport, headers);
      await get().loadMcpServers();
      return true;
    } catch (e) {
      toast.error(errText(e));
      return false;
    }
  },

  toggleMcp: async (id, enable) => {
    try {
      await configApi.toggleMcpServer(id, enable);
      await get().loadMcpServers();
      return true;
    } catch (e) {
      toast.error(errText(e));
      return false;
    }
  },

  deleteMcp: async (id) => {
    try {
      await configApi.deleteMcpServer(id);
      await get().loadMcpServers();
      return true;
    } catch (e) {
      toast.error(errText(e));
      return false;
    }
  },

  /* ---- 权限规则（三级作用域：全局 / 本项目 / 本会话） ---- */

  loadVisibleRules: async (sessionId, projectId) => {
    set({ loadingPerms: true });
    try {
      set({ permRules: await configApi.listVisiblePermRules(sessionId, projectId) });
    } catch (e) {
      toast.error(`加载权限规则失败：${errText(e)}`);
    } finally {
      set({ loadingPerms: false });
    }
  },

  loadGlobalRules: async () => {
    try {
      set({ globalRules: await configApi.listGlobalPermRules() });
    } catch (e) {
      toast.error(`加载全局规则失败：${errText(e)}`);
    }
  },

  savePermRule: async (id, scope, projectId, sessionId, tool, pattern, action, sort) => {
    try {
      await configApi.savePermRule(id, scope, projectId, sessionId, tool, pattern, action, sort);
      return true;
    } catch (e) {
      toast.error(errText(e));
      return false;
    }
  },

  deletePermRule: async (id) => {
    try {
      await configApi.deletePermRule(id);
      return true;
    } catch (e) {
      toast.error(errText(e));
      return false;
    }
  },
}));
