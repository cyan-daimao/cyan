import { create } from 'zustand';
import type {
  AgentEvent,
  ApprovalDecision,
  ApprovalState,
  ChangeView,
  ChatNode,
  RuleScope,
  RunState,
  TodoDTO,
  Tokens,
} from '../types';
import { approve, interruptRun, rollbackChange, sendTask } from '../services/agent';
import { errText, toast } from '../utils/feedback';
import { fmtTokens } from '../utils/format';
import { newNodeId, useSessionStore } from './sessionStore';
import { useProjectStore } from './projectStore';
import { useConfigStore } from './configStore';

/**
 * Agent 运行阶段（派生展示状态）：
 * thinking（等待首个响应/工具间思考）→ streaming（文本/思考流出）→ tool（工具执行）
 * → approval（等待审批）→ thinking …；run_end / 出错 / 中断归 null。
 */
export type AgentPhase = 'thinking' | 'streaming' | 'tool' | 'approval' | null;

/** 审批请求中（等待用户决断）的载荷 */
interface PendingApproval {
  callId: string;
  tool: string;
  arg: string;
  reason: string;
}

/** 单个会话的运行标记（驱动会话列表右侧的 loading / 完成指示） */
export type SessionRunFlag = 'running' | 'waiting_approval' | 'done' | 'error';

interface AgentState {
  /** 会话运行状态机（PRD 8.1）——当前激活会话的视图态 */
  runState: RunState;
  /** 正在运行的会话 id（后端 i64）——当前激活会话视角 */
  runSessionId: number | null;
  /** 全部会话的运行标记（跨项目并发运行，sessionId → 状态） */
  sessionRuns: Record<number, SessionRunFlag>;
  /** 各会话运行开始时间（sessionId → Date.now()，内存态，运行结束清除） */
  runStartedAt: Record<number, number>;
  todos: TodoDTO[];
  /** 本次会话的文件变更（含 diff 快照） */
  changes: ChangeView[];
  /** 上下文占用百分比（≥80 前端警示） */
  ctxPercent: number;
  tokens: Tokens;
  pendingApproval: PendingApproval | null;
  /** 当前运行阶段（驱动「正在思考」loading 气泡） */
  phase: AgentPhase;

  /** 发送任务；返回是否已被受理（受理后输入框可清空）。
   *  opts.skipAppend：编辑即截断重发场景，后端不再 append 用户消息，前端也不重复插入气泡 */
  send: (text: string, opts?: { skipAppend?: boolean }) => Promise<boolean>;
  /** 中断当前运行（Esc / 停止键） */
  interrupt: () => Promise<void>;
  /** 审批决断（允许一次 / 总是允许 / 拒绝）；alwaysScope 为「总是允许」的规则作用域 */
  decide: (callId: string, decision: ApprovalDecision, alwaysScope?: RuleScope) => Promise<void>;
  /** checkpoint 回滚 */
  rollback: (changeId: number) => Promise<void>;
  /** 切换会话后同步运行态展示 */
  resetForSession: (summary?: { ctx: number; tokens: Tokens }) => void;
  /** 打开会话后清除其完成/出错标记 */
  clearSessionFlag: (sessionId: number) => void;
  /** agent:event 统一入口（TECH_DESIGN 2.4） */
  onAgentEvent: (evt: AgentEvent) => void;
}

const EMPTY_TOKENS: Tokens = { input: 0, output: 0 };

/** 标记是否处于运行中（含等待审批） */
const isBusyFlag = (f?: SessionRunFlag) => f === 'running' || f === 'waiting_approval';

export const useAgentStore = create<AgentState>((set, get) => ({
  runState: 'idle',
  runSessionId: null,
  sessionRuns: {},
  runStartedAt: {},
  todos: [],
  changes: [],
  ctxPercent: 0,
  tokens: EMPTY_TOKENS,
  pendingApproval: null,
  phase: null,

  send: async (text, opts) => {
    const trimmed = text.trim();
    const skipAppend = opts?.skipAppend === true;
    if (!trimmed) {
      toast.warning('请输入任务描述');
      return false;
    }
    const project = useProjectStore.getState().current;
    if (!project) {
      toast.warning('请先打开一个项目');
      return false;
    }
    // 无激活会话时自动新建（PRD 10：发送需非空文本，空闲时发送进入 running）
    let sessionId = useSessionStore.getState().activeId;
    if (sessionId === null) {
      sessionId = await useSessionStore.getState().createSession(project.path);
      if (sessionId === null) return false;
    }
    // 并发语义：仅拦截「当前会话」的运行，其他会话/项目的任务互不阻塞
    if (isBusyFlag(get().sessionRuns[sessionId])) {
      toast.warning('当前会话已有任务运行中，请先停止或切换会话');
      return false;
    }
    const cfg = useConfigStore.getState();
    // 模型取值顺序：会话级偏好 → 全局选中 → 默认模型 → 第一个启用的（skipAppend 重发同序）
    const model =
      cfg.sessionModels[sessionId] ??
      cfg.activeModel ??
      cfg.models.find((m) => m.isDefault && m.status === 'enabled')?.name ??
      cfg.models.find((m) => m.status === 'enabled')?.name;
    if (!model) {
      toast.warning('请先在「设置 - 模型配置」中添加并启用模型');
      return false;
    }
    if (!skipAppend) {
      useSessionStore.getState().pushNode({ id: newNodeId(), kind: 'user', text: trimmed });
    }
    set({
      runState: 'running',
      runSessionId: sessionId,
      phase: 'thinking',
      sessionRuns: { ...get().sessionRuns, [sessionId]: 'running' },
      runStartedAt: { ...get().runStartedAt, [sessionId]: Date.now() },
    });
    try {
      await sendTask(sessionId, trimmed, model, cfg.permMode, cfg.disabledTools, skipAppend);
      return true;
    } catch (e) {
      const { [sessionId]: _drop, ...rest } = get().sessionRuns;
      const { [sessionId]: _dropT, ...restStarted } = get().runStartedAt;
      set({
        runState: 'idle',
        runSessionId: null,
        phase: null,
        sessionRuns: rest,
        runStartedAt: restStarted,
      });
      useSessionStore.getState().pushNode({
        id: newNodeId(),
        kind: 'system',
        text: `⚠️ 任务发送失败：${errText(e)}`,
      });
      toast.error(`任务发送失败：${errText(e)}`);
      return false;
    }
  },

  interrupt: async () => {
    const { runState, runSessionId } = get();
    if ((runState !== 'running' && runState !== 'waiting_approval') || runSessionId === null) return;
    try {
      await interruptRun(runSessionId);
    } catch (e) {
      toast.error(`中断失败：${errText(e)}`);
    }
  },

  decide: async (callId, decision, alwaysScope) => {
    const { runSessionId } = get();
    if (runSessionId === null) return;
    try {
      await approve(runSessionId, callId, decision, alwaysScope);
      // 卡片状态更新等待 approval_resolved 事件，保证与后端一致
    } catch (e) {
      toast.error(`审批提交失败：${errText(e)}`);
    }
  },

  rollback: async (changeId) => {
    const sessionId = useSessionStore.getState().activeId;
    if (sessionId === null) return;
    try {
      await rollbackChange(sessionId, changeId);
      set({ changes: get().changes.filter((c) => c.changeId !== changeId) });
      toast.success('已回滚到该 checkpoint 之前的状态');
    } catch (e) {
      toast.error(`回滚失败：${errText(e)}`);
    }
  },

  resetForSession: (summary) => {
    // 视图态对齐到新激活会话：该会话正在运行时恢复 running 视图（停止键/思考气泡）
    const sid = useSessionStore.getState().activeId;
    const flag = sid !== null ? get().sessionRuns[sid] : undefined;
    const busy = isBusyFlag(flag);
    set({
      todos: [],
      changes: [],
      pendingApproval: null,
      runState: busy ? (flag as RunState) : 'idle',
      runSessionId: busy ? sid : null,
      // 等待审批的会话切回来显示 approval 阶段（审批卡已从 DB 还原），而非误导性的思考中
      phase: flag === 'waiting_approval' ? 'approval' : busy ? 'thinking' : null,
      ctxPercent: summary?.ctx ?? 0,
      tokens: summary?.tokens ?? EMPTY_TOKENS,
    });
  },

  clearSessionFlag: (sessionId) => {
    const flag = get().sessionRuns[sessionId];
    if (flag !== 'done' && flag !== 'error') return;
    const { [sessionId]: _drop, ...rest } = get().sessionRuns;
    set({ sessionRuns: rest });
  },

  onAgentEvent: (evt) => {
    const ss = useSessionStore.getState();
    // 多会话并发：消息流与视图态只对「激活会话」生效；运行标记对所有会话生效
    const isActive = evt.sessionId === ss.activeId;
    const mark = (flag: SessionRunFlag) => {
      set({ sessionRuns: { ...get().sessionRuns, [evt.sessionId]: flag } });
      // 兜底记录开始时间（例如后端侧发起的运行）；已存在则不覆盖
      if (flag === 'running' && get().runStartedAt[evt.sessionId] === undefined) {
        set({ runStartedAt: { ...get().runStartedAt, [evt.sessionId]: Date.now() } });
      }
    };
    switch (evt.type) {
      case 'text_delta':
        if (!isActive) break;
        set({ phase: 'streaming' });
        ss.appendDelta(evt.delta);
        break;
      case 'thinking_delta':
        if (!isActive) break;
        set({ phase: 'streaming' });
        ss.appendThinkingDelta(evt.delta);
        break;
      case 'tool_start':
        if (!isActive) break;
        set({ phase: 'tool' });
        ss.endStreaming();
        ss.pushNode({
          id: newNodeId(),
          kind: 'tool',
          callId: evt.callId,
          tool: evt.tool,
          arg: evt.arg,
          status: 'running',
        });
        break;
      case 'tool_end':
        if (!isActive) break;
        set({ phase: 'thinking' });
        ss.updateTool(evt.callId, {
          status: evt.status,
          output: evt.output,
          note: evt.note,
          // 后端不携带 outputType，按 unified diff 头推断
          outputType: evt.output.startsWith('@@') ? 'diff' : 'code',
          // 执行结束：清空实时缓冲，照常用最终 output 渲染
          liveOutput: undefined,
        });
        break;
      case 'tool_delta':
        // 工具执行中实时输出（Bash 等），只对激活会话更新
        if (!isActive) break;
        ss.appendToolDelta(evt.callId, evt.delta);
        break;
      case 'approval_required':
        mark('waiting_approval');
        if (!isActive) break;
        ss.endStreaming();
        set({
          runState: 'waiting_approval',
          phase: 'approval',
          pendingApproval: { callId: evt.callId, tool: evt.tool, arg: evt.arg, reason: evt.reason },
        });
        ss.pushNode({
          id: newNodeId(),
          kind: 'approval',
          callId: evt.callId,
          tool: evt.tool,
          arg: evt.arg,
          reason: evt.reason,
          state: 'pending',
        });
        break;
      case 'approval_resolved': {
        mark('running');
        const map: Record<string, Exclude<ApprovalState, 'pending'>> = {
          once: 'allowed',
          always: 'always',
          auto: 'auto',
          reject: 'rejected',
          abort: 'rejected',
        };
        if (isActive) {
          ss.updateApproval(evt.callId, map[evt.decision] ?? 'rejected');
          set({ pendingApproval: null, runState: 'running', phase: 'thinking' });
        }
        if (evt.decision === 'always') {
          // 「总是允许」后端已按所选作用域落库，刷新可见规则与全局规则
          const project = useProjectStore.getState().current;
          if (project) {
            void useConfigStore.getState().loadVisibleRules(evt.sessionId, project.id);
          }
          void useConfigStore.getState().loadGlobalRules();
          toast.success('已加入白名单，后续自动放行');
        }
        break;
      }
      case 'todo_update':
        if (!isActive) break;
        set({ todos: evt.todos });
        break;
      case 'change_add': {
        if (!isActive) break;
        // 从对应 Edit/Write 工具卡捕获 diff 快照（验收 11：查看内容与原 Edit 卡一致）
        const toolNode = [...ss.messages]
          .reverse()
          .find(
            (n): n is Extract<ChatNode, { kind: 'tool' }> =>
              n.kind === 'tool' && n.arg === evt.change.filePath && n.outputType === 'diff',
          );
        const view: ChangeView = { ...evt.change, diff: toolNode?.output ?? '' };
        set({ changes: [...get().changes, view] });
        break;
      }
      case 'ctx_update':
        if (!isActive) break;
        set({ ctxPercent: evt.ctxPercent, tokens: evt.tokens });
        break;
      case 'compacted':
        if (!isActive) break;
        ss.pushNode({
          id: newNodeId(),
          kind: 'system',
          text: `🗜 上下文已自动压缩：${evt.summary}`,
        });
        break;
      case 'run_continued':
        if (!isActive) break;
        ss.pushNode({
          id: newNodeId(),
          kind: 'system',
          text: `⏳ 已执行 25 轮工具调用，任务未完成，自动继续执行（第 ${evt.round} 次续跑）`,
        });
        break;
      case 'run_end': {
        mark(evt.result === 'error' ? 'error' : 'done');
        // 运行结束（done/aborted/error）清除计时
        const { [evt.sessionId]: _dropT, ...restStarted } = get().runStartedAt;
        set({ runStartedAt: restStarted });
        // 运行结束刷新会话列表（标题 / ctx / tokens 已变化）
        const project = useProjectStore.getState().current;
        if (project) {
          void useSessionStore
            .getState()
            .loadSessions(project.path, useSessionStore.getState().searchKw || undefined);
        }
        if (!isActive) {
          if (evt.result === 'done') toast.success('后台会话任务已完成');
          else if (evt.result === 'error') toast.error('后台会话任务出错');
          break;
        }
        ss.endStreaming();
        set({
          pendingApproval: null,
          runState: evt.result === 'error' ? 'error' : 'idle',
          phase: null,
        });
        if (evt.result === 'done') {
          ss.pushNode({
            id: newNodeId(),
            kind: 'system',
            text: `任务完成 · 本次消耗 ↑ ${fmtTokens(evt.usage.input)} ↓ ${fmtTokens(evt.usage.output)} tokens`,
          });
        } else if (evt.result === 'aborted') {
          ss.pushNode({ id: newNodeId(), kind: 'system', text: '⏹ 已由用户中断' });
        } else {
          ss.pushNode({
            id: newNodeId(),
            kind: 'system',
            text: `⚠️ 运行出错：${evt.message ?? '未知错误'}`,
          });
          toast.error('Agent 运行出错，请重试');
        }
        break;
      }
    }
  },
}));
