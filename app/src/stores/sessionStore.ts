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
import { useConfigStore } from './configStore';
import { useProjectStore } from './projectStore';
import { useAgentStore } from './agentStore';

/** 聊天窗口默认页大小：打开会话/向前翻页每次加载的消息条数 */
export const MESSAGE_PAGE_SIZE = 60;

/** 消息节点本地 id 生成（仅前端渲染用，与后端消息 id 无关） */
let nodeSeq = 0;
export const newNodeId = () => `n${(nodeSeq += 1)}`;

/* text_delta / thinking_delta 流式缓冲：token 级频率的事件用 requestAnimationFrame 节流合并。
 * 两条缓冲独立（text 与 thinking 分开），避免相互覆盖。 */
let textBuf = '';
let textRaf: number | null = null;
let thinkBuf = '';
let thinkRaf: number | null = null;
/* tool_delta 流式缓冲（按 callId 分组）：Bash 长输出时 delta 频率可达 token 级，
 * 与 text/thinking 同样用 rAF 节流合并，每帧最多 set 一次。 */
let toolBuf = new Map<string, string>();
let toolRaf: number | null = null;
/** 工具实时输出缓冲上限 50KB，超出保留尾部（截头） */
const TOOL_LIVE_CAP = 50 * 1024;

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
  toolBuf.clear();
  if (toolRaf !== null) {
    cancelAnimationFrame(toolRaf);
    toolRaf = null;
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
  /** 激活会话的消息流（尾部窗口；更早历史经 loadOlder 向前加载） */
  messages: ChatNode[];
  /** 历史是否还有更早消息（窗口分页游标） */
  hasMore: boolean;
  /** 下一页游标：窗口内最早一条的后端 seq（空窗口为 null） */
  oldestSeq: number | null;
  /** 是否正在向前加载历史 */
  loadingOlder: boolean;
  /** 会话搜索关键词 */
  searchKw: string;
  loadingList: boolean;
  loadingMessages: boolean;

  setSearchKw: (kw: string) => void;
  loadSessions: (projectPath: string, keyword?: string) => Promise<void>;
  openSession: (sessionId: number) => Promise<SessionDTO | null>;
  /** 向前加载一页历史（prepend，头部 seq 为游标）；无更多/加载中返回 null */
  loadOlder: () => Promise<number | null>;
  createSession: (projectPath: string) => Promise<number | null>;
  deleteSession: (sessionId: number) => Promise<void>;
  /** 重命名会话标题（同步刷新 sessions 列表；otherSessions 缓存由调用方同步） */
  renameSession: (sessionId: number, title: string) => Promise<boolean>;
  /** 清空会话上下文（/clear）：硬删全部消息，统计归零；返回清除的消息数 */
  clearSession: (sessionId: number) => Promise<number | null>;
  /** 清空会话上下文（/clear）：硬删全部消息，统计归零；返回清除的消息数 */
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
    patch: {
      status: ToolStatus;
      output?: string;
      note?: string;
      outputType?: 'code' | 'diff' | 'text';
      liveOutput?: string;
    },
  ) => void;
  /** tool_delta：把 stdout/stderr 增量 append 到运行中工具卡的实时缓冲（rAF 节流合并，50KB 截头） */
  appendToolDelta: (callId: string, delta: string) => void;
  flushToolDelta: () => void;
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
  hasMore: false,
  oldestSeq: null,
  loadingOlder: false,
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
      // 打开即只拉尾部窗口（约一屏半），更早历史由用户上滚时 loadOlder 增量加载；
      // ctx/token/模型偏好随分页响应一并返回，单次往返完成装配
      const page = await sessionApi.listMessages(sessionId, undefined, MESSAGE_PAGE_SIZE);
      const messages = page.messages
        .map(dtoToNode)
        .filter((n): n is ChatNode => n !== null);
      set({
        activeId: sessionId,
        messages,
        hasMore: page.hasMore,
        oldestSeq: page.oldestSeq ?? null,
      });
      // 播种会话级模型偏好（null 则清除该键，跟随全局）
      useConfigStore.getState().seedSessionModel(sessionId, page.preferredModel);
      // 组装视图态需要的会话摘要（与 SessionDTO 同形）
      const summary = get().sessions.find((s) => s.id === sessionId);
      const dto: SessionDTO = {
        id: sessionId,
        projectId: 0,
        title: summary?.title ?? '',
        ctx: page.ctx,
        tokens: page.tokens,
        messages: [],
        createdAt: '',
        updatedAt: '',
      };
      return dto;
    } catch (e) {
      toast.error(`打开会话失败：${errText(e)}`);
      return null;
    } finally {
      set({ loadingMessages: false });
    }
  },

  loadOlder: async () => {
    const { activeId, oldestSeq, loadingOlder, hasMore } = get();
    if (!hasMore || loadingOlder || activeId === null || oldestSeq === null) return null;
    set({ loadingOlder: true });
    try {
      const page = await sessionApi.listMessages(activeId, oldestSeq, MESSAGE_PAGE_SIZE);
      if (get().activeId !== activeId) return null; // 期间已切换会话
      const older = page.messages
        .map(dtoToNode)
        .filter((n): n is ChatNode => n !== null);
      if (older.length > 0) {
        set({ messages: [...older, ...get().messages], hasMore: page.hasMore, oldestSeq: page.oldestSeq ?? null });
      } else {
        set({ hasMore: false });
      }
      return older.length;
    } catch (e) {
      toast.error(`加载历史消息失败：${errText(e)}`);
      return null;
    } finally {
      set({ loadingOlder: false });
    }
  },

  createSession: async (projectPath) => {
    clearDeltaBuffer();
    try {
      const dto = await sessionApi.createSession(projectPath);
      set({ activeId: dto.id, messages: [] });
      // 新会话无偏好，确保本地 map 无残留
      useConfigStore.getState().seedSessionModel(dto.id, dto.preferredModel ?? null);
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

  renameSession: async (sessionId, title) => {
    const next = title.trim();
    if (!next) {
      toast.warning('标题不能为空');
      return false;
    }
    if (next.length > 80) {
      toast.warning('标题长度不能超过 80 字符');
      return false;
    }
    const before = get().sessions.find((s) => s.id === sessionId);
    // 幂等：与原标题一致（含 trim）跳过网络
    if (before && before.title.trim() === next) return true;
    try {
      await sessionApi.renameSession(sessionId, next);
      // 本地同步：避免 reload 整个项目列表
      set({
        sessions: get().sessions.map((s) =>
          s.id === sessionId
            ? { ...s, title: next, updatedAt: new Date().toISOString().replace('T', ' ').slice(0, 19) }
            : s,
        ),
      });
      return true;
    } catch (e) {
      toast.error(`重命名失败：${errText(e)}`);
      return false;
    }
  },

  clearSession: async (sessionId) => {
    try {
      const removed = await sessionApi.clearSession(sessionId);
      clearDeltaBuffer();
      const { activeId } = get();
      if (activeId === sessionId) {
        set({ messages: [] });
        // 重置当前会话的 token/ctx 显示
        useAgentStore.getState().resetForSession();
      }
      await get().loadSessions(useProjectStore.getState().current?.path ?? '', get().searchKw || undefined);
      return removed;
    } catch (e) {
      toast.error(`清空上下文失败：${errText(e)}`);
      return null;
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
    get().flushToolDelta();
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
    // 先落地缓冲中的实时增量，再应用 tool_end 补丁（最终 output 覆盖 liveOutput）
    get().flushToolDelta();
    set({
      messages: get().messages.map((n) =>
        n.kind === 'tool' && n.callId === callId ? { ...n, ...patch } : n,
      ),
    });
  },

  appendToolDelta: (callId, delta) => {
    toolBuf.set(callId, (toolBuf.get(callId) ?? '') + delta);
    if (toolRaf === null) {
      toolRaf = requestAnimationFrame(() => {
        toolRaf = null;
        get().flushToolDelta();
      });
    }
  },

  flushToolDelta: () => {
    if (toolRaf !== null) {
      cancelAnimationFrame(toolRaf);
      toolRaf = null;
    }
    if (toolBuf.size === 0) return;
    const buf = toolBuf;
    toolBuf = new Map();
    set({
      messages: get().messages.map((n) => {
        if (n.kind !== 'tool' || n.status !== 'running') return n;
        const delta = buf.get(n.callId);
        if (delta === undefined) return n;
        let live = (n.liveOutput ?? '') + delta;
        if (live.length > TOOL_LIVE_CAP) live = live.slice(live.length - TOOL_LIVE_CAP);
        return { ...n, liveOutput: live };
      }),
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
    // 编辑截断返回剩余全量消息：仍按尾窗收敛，避免超长会话编辑后 DOM 回退全量；
    // dto 是真实全量（截断语义），游标按截断后头部的后端 seq 重算
    const windowed = dto.messages.slice(-MESSAGE_PAGE_SIZE);
    set({
      activeId: dto.id,
      messages: windowed.map(dtoToNode).filter((n): n is ChatNode => n !== null),
      hasMore: dto.messages.length > MESSAGE_PAGE_SIZE,
      oldestSeq: windowed.length > 0 ? windowed[0].seq : null,
    });
    // 编辑消息后同步会话级模型偏好
    useConfigStore.getState().seedSessionModel(dto.id, dto.preferredModel);
  },

  resolveMessageId: async (nodeId) => {
    // 持久化消息的节点 id 即后端消息 id（dtoToNode 用 String(m.id)）
    const direct = Number(nodeId);
    if (nodeId && Number.isInteger(direct)) return direct;
    // 运行期本地节点（n1/n2…）：拉取持久化会话，按 user/assistant 序号从尾部对齐
    // （本地节点必然位于尾部；头部对齐在窗口化丢弃早期消息后会错位）
    const { activeId, messages } = get();
    if (activeId === null) return null;
    try {
      const dto = await sessionApi.getSession(activeId);
      const front = messages.filter((m) => m.kind === 'user' || m.kind === 'assistant');
      const pos = front.findIndex((m) => m.id === nodeId);
      if (pos < 0) return null;
      const back = dto.messages.filter((m) => m.kind === 'user' || m.kind === 'assistant');
      const fromEnd = front.length - 1 - pos;
      const backPos = back.length - 1 - fromEnd;
      return backPos >= 0 ? back[backPos].id : null;
    } catch {
      return null;
    }
  },
}));
