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

/* text_delta / thinking_delta 流式缓冲：token 级频率的事件用 requestAnimationFrame 节流合并。
 * 两条缓冲独立（text 与 thinking 分开），避免相互覆盖。 */
let textBuf = '';
let textRaf: number | null = null;
let thinkBuf = '';
let thinkRaf: number | null = null;

function clearDeltaBuffer() {
  textBuf = '';
  thinkBuf = '';
  if (textRaf !== null) {
    cancelAnimationFrame(textRaf);
    textRaf = null;
  }
  if (thinkRaf !== null) {
    cancelAnimationFrame(thinkRaf);
    thinkRaf = null;
  }
}

/** 后端持久化的审批 decision 字符串 → 前端审批卡状态 */
function decisionToState(d: unknown): ApprovalState {
  switch (d) {
    case 'pending':
      // 等待决断中的审批（切换会话后从 DB 还原）
      return 'pending';
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
      return {
        id,
        kind: 'assistant',
        text: String(p.text ?? ''),
        thinking: typeof p.thinking === 'string' && p.thinking ? p.thinking : undefined,
      };
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
        reason: String(p.reason ?? ''),
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
  appendThinkingDelta: (delta: string) => void;
  flushDelta: () => void;
  flushThinking: () => void;
  endStreaming: () => void;
  updateTool: (
    callId: string,
    patch: { status: ToolStatus; output?: string; note?: string; outputType?: 'code' | 'diff' | 'text' },
  ) => void;
  updateApproval: (callId: string, state: Exclude<ApprovalState, 'pending'>) => void;

  /* ---- 行内编辑（编辑即截断重发） ---- */
  /** 用后端返回的完整会话替换当前消息列表 */
  replaceMessages: (dto: SessionDTO) => void;
  /** 解析节点的后端消息 id：持久化节点直接取；运行期本地节点按 user/assistant 序号对齐 */
  resolveMessageId: (nodeId: string) => Promise<number | null>;
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
    get().flushThinking();
    set({ messages: [...get().messages, node] });
  },

  appendDelta: (delta) => {
    textBuf += delta;
    if (textRaf === null) {
      textRaf = requestAnimationFrame(() => {
        textRaf = null;
        get().flushDelta();
      });
    }
  },

  appendThinkingDelta: (delta) => {
    thinkBuf += delta;
    if (thinkRaf === null) {
      thinkRaf = requestAnimationFrame(() => {
        thinkRaf = null;
        get().flushThinking();
      });
    }
  },

  flushDelta: () => {
    if (textRaf !== null) {
      cancelAnimationFrame(textRaf);
      textRaf = null;
    }
    if (!textBuf) return;
    const text = textBuf;
    textBuf = '';
    const msgs = [...get().messages];
    const last = msgs[msgs.length - 1];
    if (last && last.kind === 'assistant' && last.streaming) {
      msgs[msgs.length - 1] = { ...last, text: last.text + text };
    } else {
      msgs.push({ id: newNodeId(), kind: 'assistant', text, streaming: true });
    }
    set({ messages: msgs });
  },

  flushThinking: () => {
    if (thinkRaf !== null) {
      cancelAnimationFrame(thinkRaf);
      thinkRaf = null;
    }
    if (!thinkBuf) return;
    const thinking = thinkBuf;
    thinkBuf = '';
    const msgs = [...get().messages];
    const last = msgs[msgs.length - 1];
    if (last && last.kind === 'assistant' && last.streaming) {
      msgs[msgs.length - 1] = { ...last, thinking: (last.thinking ?? '') + thinking };
    } else {
      msgs.push({ id: newNodeId(), kind: 'assistant', text: '', thinking, streaming: true });
    }
    set({ messages: msgs });
  },

  endStreaming: () => {
    get().flushDelta();
    get().flushThinking();
    const msgs = [...get().messages];
    const last = msgs[msgs.length - 1];
    if (last && last.kind === 'assistant' && last.streaming) {
      msgs[msgs.length - 1] = { ...last, streaming: false };
      set({ messages: msgs });
    }
  },

  updateTool: (callId, patch) => {
    get().flushDelta();
    get().flushThinking();
    set({
      messages: get().messages.map((n) =>
        n.kind === 'tool' && n.callId === callId ? { ...n, ...patch } : n,
      ),
    });
  },

  updateApproval: (callId, state) => {
    get().flushDelta();
    get().flushThinking();
    set({
      messages: get().messages.map((n) =>
        n.kind === 'approval' && n.callId === callId ? { ...n, state } : n,
      ),
    });
  },

  replaceMessages: (dto) => {
    clearDeltaBuffer();
    set({
      activeId: dto.id,
      messages: dto.messages.map(dtoToNode).filter((n): n is ChatNode => n !== null),
    });
  },

  resolveMessageId: async (nodeId) => {
    // 持久化消息的节点 id 即后端消息 id（dtoToNode 用 String(m.id)）
    const direct = Number(nodeId);
    if (nodeId && Number.isInteger(direct)) return direct;
    // 运行期本地节点（n1/n2…）：拉取持久化会话，按 user/assistant 序号对齐
    const { activeId, messages } = get();
    if (activeId === null) return null;
    try {
      const dto = await sessionApi.getSession(activeId);
      const front = messages.filter((m) => m.kind === 'user' || m.kind === 'assistant');
      const pos = front.findIndex((m) => m.id === nodeId);
      if (pos < 0) return null;
      const back = dto.messages.filter((m) => m.kind === 'user' || m.kind === 'assistant');
      return pos < back.length ? back[pos].id : null;
    } catch {
      return null;
    }
  },
}));
