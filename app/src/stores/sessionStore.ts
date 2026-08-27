import { create } from 'zustand';
import type {
  ApprovalState,
  ChatNode,
  MessageDTO,
  SessionDTO,
  SessionSummaryDTO,
  ToolStatus,
} from '../types';
import * as sessionApi from '../services/session';
import { errText, toast } from '../utils/feedback';

/** 消息节点本地 id 生成（仅前端渲染用，与后端消息 id 无关） */
let nodeSeq = 0;
export const newNodeId = () => `n${(nodeSeq += 1)}`;

/* text_delta 流式缓冲：token 级频率的事件用 requestAnimationFrame 节流合并（TECH_DESIGN 2.4） */
let deltaBuf = '';
let rafId: number | null = null;

function clearDeltaBuffer() {
  deltaBuf = '';
  if (rafId !== null) {
    cancelAnimationFrame(rafId);
    rafId = null;
  }
}

/** 后端持久化的审批 decision 字符串 → 前端审批卡状态 */
function decisionToState(d: unknown): Exclude<ApprovalState, 'pending'> {
  switch (d) {
    case 'once':
      return 'allowed';
    case 'always':
      return 'always';
    case 'auto':
      return 'auto';
    default:
      // reject / abort / 未知
      return 'rejected';
  }
}

/** 输出文本推断渲染类型（后端 tool payload / tool_end 事件均不携带 outputType） */
function inferOutputType(output: string): 'code' | 'diff' {
  return output.startsWith('@@') ? 'diff' : 'code';
}

/**
 * MessageDTO → 前端渲染节点。
 * 后端 payload 已是解析后的 JSON 对象（serde_json::Value），无需再 parse。
 */
function dtoToNode(m: MessageDTO): ChatNode | null {
  const p = m.payload ?? {};
  const id = String(m.id);
  switch (m.kind) {
    case 'user':
      return { id, kind: 'user', text: String(p.text ?? '') };
    case 'assistant':
      return { id, kind: 'assistant', text: String(p.text ?? '') };
    case 'system':
      return { id, kind: 'system', text: String(p.text ?? '') };
    case 'tool': {
      const output = String(p.output ?? '');
      return {
        id,
        kind: 'tool',
        callId: String(p.callId ?? ''),
        tool: String(p.tool ?? ''),
        arg: String(p.arg ?? ''),
        status: (p.status ?? 'ok') as ToolStatus,
        outputType: inferOutputType(output),
        output,
        note: p.note as string | undefined,
      };
    }
    case 'approval':
      return {
        id,
        kind: 'approval',
        callId: String(p.callId ?? ''),
        tool: String(p.tool ?? ''),
        arg: String(p.arg ?? ''),
        reason: '',
        state: decisionToState(p.decision),
      };
    default:
      return null;
  }
}

interface SessionState {
  /** 当前项目的会话列表 */
  sessions: SessionSummaryDTO[];
  /** 当前激活会话 id（后端 i64） */
  activeId: number | null;
  /** 激活会话的消息流 */
  messages: ChatNode[];
  /** 会话搜索关键词 */
  searchKw: string;
  loadingList: boolean;
  loadingMessages: boolean;

  setSearchKw: (kw: string) => void;
  loadSessions: (projectPath: string, keyword?: string) => Promise<void>;
  openSession: (sessionId: number) => Promise<SessionDTO | null>;
  createSession: (projectPath: string) => Promise<number | null>;
  deleteSession: (sessionId: number) => Promise<void>;
  /** 项目切换时清空会话上下文 */
  resetForProject: () => void;

  /* ---- 运行期消息流操作（供 agentStore 事件分发调用） ---- */
  pushNode: (node: ChatNode) => void;
  appendDelta: (delta: string) => void;
  flushDelta: () => void;
  endStreaming: () => void;
  updateTool: (
    callId: string,
    patch: { status: ToolStatus; output?: string; note?: string; outputType?: 'code' | 'diff' | 'text' },
  ) => void;
  updateApproval: (callId: string, state: Exclude<ApprovalState, 'pending'>) => void;
}

export const useSessionStore = create<SessionState>((set, get) => ({
  sessions: [],
  activeId: null,
  messages: [],
  searchKw: '',
  loadingList: false,
  loadingMessages: false,

  setSearchKw: (kw) => set({ searchKw: kw }),

  loadSessions: async (projectPath, keyword) => {
    set({ loadingList: true });
    try {
      const list = await sessionApi.listSessions(projectPath, keyword || undefined);
      set({ sessions: list });
    } catch (e) {
      toast.error(`加载会话列表失败：${errText(e)}`);
    } finally {
      set({ loadingList: false });
    }
  },

  openSession: async (sessionId) => {
    clearDeltaBuffer();
    set({ loadingMessages: true });
    try {
      const dto = await sessionApi.getSession(sessionId);
      const messages = dto.messages
        .map(dtoToNode)
        .filter((n): n is ChatNode => n !== null);
      set({ activeId: sessionId, messages });
      return dto;
    } catch (e) {
      toast.error(`打开会话失败：${errText(e)}`);
      return null;
    } finally {
      set({ loadingMessages: false });
    }
  },

  createSession: async (projectPath) => {
    clearDeltaBuffer();
    try {
      const dto = await sessionApi.createSession(projectPath);
      set({ activeId: dto.id, messages: [] });
      await get().loadSessions(projectPath, get().searchKw || undefined);
      return dto.id;
    } catch (e) {
      toast.error(`新建会话失败：${errText(e)}`);
      return null;
    }
  },

  deleteSession: async (sessionId) => {
    try {
      await sessionApi.deleteSession(sessionId);
      const { activeId, sessions } = get();
      const remaining = sessions.filter((s) => s.id !== sessionId);
      if (activeId === sessionId) {
        // 删除当前会话则回空状态
        clearDeltaBuffer();
        set({ sessions: remaining, activeId: null, messages: [] });
      } else {
        set({ sessions: remaining });
      }
    } catch (e) {
      toast.error(`删除会话失败：${errText(e)}`);
      throw e;
    }
  },

  resetForProject: () => {
    clearDeltaBuffer();
    set({ sessions: [], activeId: null, messages: [], searchKw: '' });
  },

  pushNode: (node) => {
    get().flushDelta();
    set({ messages: [...get().messages, node] });
  },

  appendDelta: (delta) => {
    deltaBuf += delta;
    if (rafId === null) {
      rafId = requestAnimationFrame(() => {
        rafId = null;
        get().flushDelta();
      });
    }
  },

  flushDelta: () => {
    if (rafId !== null) {
      cancelAnimationFrame(rafId);
      rafId = null;
    }
    if (!deltaBuf) return;
    const text = deltaBuf;
    deltaBuf = '';
    const msgs = [...get().messages];
    const last = msgs[msgs.length - 1];
    if (last && last.kind === 'assistant' && last.streaming) {
      msgs[msgs.length - 1] = { ...last, text: last.text + text };
    } else {
      msgs.push({ id: newNodeId(), kind: 'assistant', text, streaming: true });
    }
    set({ messages: msgs });
  },

  endStreaming: () => {
    get().flushDelta();
    const msgs = [...get().messages];
    const last = msgs[msgs.length - 1];
    if (last && last.kind === 'assistant' && last.streaming) {
      msgs[msgs.length - 1] = { ...last, streaming: false };
      set({ messages: msgs });
    }
  },

  updateTool: (callId, patch) => {
    get().flushDelta();
    set({
      messages: get().messages.map((n) =>
        n.kind === 'tool' && n.callId === callId ? { ...n, ...patch } : n,
      ),
    });
  },

  updateApproval: (callId, state) => {
    get().flushDelta();
    set({
      messages: get().messages.map((n) =>
        n.kind === 'approval' && n.callId === callId ? { ...n, state } : n,
      ),
    });
  },
}));
