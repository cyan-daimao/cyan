import { create } from 'zustand';
import type { ProjectDTO, ProjectTemplate } from '../types';
import * as projectApi from '../services/project';
import { errText, toast } from '../utils/feedback';
import { newNodeId, useSessionStore } from './sessionStore';
import { useAgentStore } from './agentStore';

interface ProjectState {
  /** 当前项目（= 工作目录） */
  current: ProjectDTO | null;
  /** 最近项目 */
  recents: ProjectDTO[];
  loading: boolean;

  /** 应用启动时加载最近项目并恢复当前项目 */
  init: () => Promise<void>;
  loadRecents: () => Promise<void>;
  /** 打开文件夹为项目并切换（含会话区联动） */
  open: (path: string) => Promise<boolean>;
  /** 新建项目（脚手架 + git init）并切换 */
  create: (name: string, parent: string, template: ProjectTemplate, gitInit: boolean) => Promise<ProjectDTO | null>;
  /** 从列表移除项目（软删记录，不删磁盘；移除当前项目则切换到下一个） */
  remove: (path: string) => Promise<boolean>;
}

/** 切换项目后的全量联动（PRD 5.3 / 验收 6） */
async function applySwitch(p: ProjectDTO, created: boolean) {
  const ss = useSessionStore.getState();
  // 当前会话有内容时插入系统消息记录切换
  if (ss.activeId && ss.messages.length > 0) {
    ss.pushNode({
      id: newNodeId(),
      kind: 'system',
      text: `📁 已${created ? '创建并' : ''}切换项目到 ${p.name}（${p.path}）· Agent 的文件与命令操作将限定在该目录内`,
    });
  }
  ss.resetForProject();
  useAgentStore.getState().resetForSession();
  await ss.loadSessions(p.path);
}

export const useProjectStore = create<ProjectState>((set, get) => ({
  current: null,
  recents: [],
  loading: false,

  init: async () => {
    set({ loading: true });
    try {
      const list = await projectApi.listProjects();
      // 后端按 lastOpened 倒序返回，首个即当前项目
      set({ recents: list, current: list[0] ?? null });
    } catch (e) {
      toast.error(`加载项目失败：${errText(e)}`);
    } finally {
      set({ loading: false });
    }
  },

  loadRecents: async () => {
    try {
      set({ recents: await projectApi.listProjects() });
    } catch (e) {
      toast.error(`加载最近项目失败：${errText(e)}`);
    }
  },

  open: async (path) => {
    try {
      const p = await projectApi.openProject(path);
      set({ current: p });
      void get().loadRecents();
      await applySwitch(p, false);
      toast.success(`当前项目：${p.name}`);
      return true;
    } catch (e) {
      toast.error(`打开项目失败：${errText(e)}`);
      return false;
    }
  },

  create: async (name, parent, template, gitInit) => {
    try {
      const p = await projectApi.createProject(name, parent, template, gitInit);
      set({ current: p });
      void get().loadRecents();
      await applySwitch(p, true);
      return p;
    } catch (e) {
      toast.error(errText(e));
      return null;
    }
  },

  remove: async (path) => {
    try {
      await projectApi.removeProject(path);
      const remaining = get().recents.filter((p) => p.path !== path);
      const wasCurrent = get().current?.path === path;
      set({ recents: remaining });
      if (wasCurrent) {
        // 移除当前项目：切到下一个最近项目，没有则回空状态
        const next = remaining[0] ?? null;
        set({ current: next });
        const ss = useSessionStore.getState();
        ss.resetForProject();
        useAgentStore.getState().resetForSession();
        if (next) await ss.loadSessions(next.path);
      }
      toast.success('已移除项目（磁盘文件未删除）');
      return true;
    } catch (e) {
      toast.error(`移除项目失败：${errText(e)}`);
      return false;
    }
  },
}));
