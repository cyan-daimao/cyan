import { create } from 'zustand';
import type {
  McpServerDTO,
  ModelDTO,
  PermAction,
  PermMode,
  PermRuleDTO,
  SaveModelRequest,
} from '../types';
import * as configApi from '../services/config';
import { errText, toast } from '../utils/feedback';

const PERM_MODE_KEY = 'cyan.permMode';
const ACTIVE_MODEL_KEY = 'cyan.activeModel';

function loadPermMode(): PermMode {
  const v = localStorage.getItem(PERM_MODE_KEY);
  return v === 'plan' || v === 'ask' || v === 'auto' ? v : 'ask';
}

interface ConfigState {
  models: ModelDTO[];
  mcpServers: McpServerDTO[];
  permRules: PermRuleDTO[];
  /** 权限模式（PRD 2.2，默认 ask） */
  permMode: PermMode;
  /** 输入区当前选中的模型名（send_task 按模型名传参） */
  activeModel: string | null;
  loadingModels: boolean;
  loadingMcp: boolean;
  loadingPerms: boolean;

  setPermMode: (mode: PermMode) => void;
  setActiveModel: (name: string) => void;
  loadAll: () => Promise<void>;

  /* ---- 模型 ---- */
  loadModels: () => Promise<void>;
  saveModel: (req: SaveModelRequest) => Promise<boolean>;
  deleteModel: (id: number) => Promise<boolean>;
  setDefault: (id: number) => Promise<boolean>;

  /* ---- MCP ---- */
  loadMcpServers: () => Promise<void>;
  saveMcpServer: (id: number | undefined, name: string, command: string) => Promise<boolean>;
  toggleMcp: (id: number, enable: boolean) => Promise<boolean>;
  deleteMcp: (id: number) => Promise<boolean>;

  /* ---- 权限规则 ---- */
  loadPermRules: () => Promise<void>;
  savePermRule: (
    id: number | undefined,
    tool: string,
    pattern: string,
    action: PermAction,
    sort: number,
  ) => Promise<boolean>;
  deletePermRule: (id: number) => Promise<boolean>;
}

export const useConfigStore = create<ConfigState>((set, get) => ({
  models: [],
  mcpServers: [],
  permRules: [],
  permMode: loadPermMode(),
  activeModel: localStorage.getItem(ACTIVE_MODEL_KEY),
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

  loadAll: async () => {
    await Promise.all([get().loadModels(), get().loadMcpServers(), get().loadPermRules()]);
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

  saveMcpServer: async (id, name, command) => {
    try {
      await configApi.saveMcpServer(id, name, command);
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

  /* ---- 权限规则 ---- */

  loadPermRules: async () => {
    set({ loadingPerms: true });
    try {
      set({ permRules: await configApi.listPermRules() });
    } catch (e) {
      toast.error(`加载权限规则失败：${errText(e)}`);
    } finally {
      set({ loadingPerms: false });
    }
  },

  savePermRule: async (id, tool, pattern, action, sort) => {
    try {
      await configApi.savePermRule(id, tool, pattern, action, sort);
      await get().loadPermRules();
      return true;
    } catch (e) {
      toast.error(errText(e));
      return false;
    }
  },

  deletePermRule: async (id) => {
    try {
      await configApi.deletePermRule(id);
      await get().loadPermRules();
      return true;
    } catch (e) {
      toast.error(errText(e));
      return false;
    }
  },
}));
