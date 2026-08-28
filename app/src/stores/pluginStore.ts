import { create } from 'zustand';
import type { PluginDTO } from '../types';
import * as pluginApi from '../services/plugin';
import { errText, toast } from '../utils/feedback';

interface PluginState {
  plugins: PluginDTO[];
  loading: boolean;
  /** 是否已加载过（避免面板反复拉取） */
  loaded: boolean;

  load: (force?: boolean) => Promise<void>;
  /** 安装（zip / 目录路径），成功后刷新，返回安装结果 */
  install: (sourcePath: string) => Promise<PluginDTO | null>;
  /** 启用/禁用，成功后刷新 */
  toggle: (id: number, enable: boolean) => Promise<boolean>;
  /** 卸载（连带摘除内容物），成功后刷新 */
  remove: (id: number) => Promise<boolean>;
}

export const usePluginStore = create<PluginState>((set, get) => ({
  plugins: [],
  loading: false,
  loaded: false,

  load: async (force) => {
    if (!force && get().loaded) return;
    set({ loading: true });
    try {
      set({ plugins: await pluginApi.listPlugins(), loaded: true });
    } catch (e) {
      toast.error(`加载插件列表失败：${errText(e)}`);
    } finally {
      set({ loading: false });
    }
  },

  install: async (sourcePath) => {
    try {
      const dto = await pluginApi.installPlugin(sourcePath);
      await get().load(true);
      return dto;
    } catch (e) {
      toast.error(`安装失败：${errText(e)}`);
      return null;
    }
  },

  toggle: async (id, enable) => {
    try {
      await pluginApi.togglePlugin(id, enable);
      await get().load(true);
      return true;
    } catch (e) {
      toast.error(errText(e));
      return false;
    }
  },

  remove: async (id) => {
    try {
      await pluginApi.deletePlugin(id);
      await get().load(true);
      return true;
    } catch (e) {
      toast.error(errText(e));
      return false;
    }
  },
}));
