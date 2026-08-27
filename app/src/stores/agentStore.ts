import { create } from 'zustand';
import type {
  AgentEvent,
  ApprovalDecision,
  ApprovalState,
  ChangeView,
  ChatNode,
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

interface AgentState {
  /** 会话运行状态机（PRD 8.1） */
  runState: RunState;
  /** 正在运行的会话 id（后端 i64） */
  runSessionId: number | null;
  todos: TodoDTO[];
  /** 本次会话的文件变更（含 diff 快照） */
  changes: ChangeView[];
  /** 上下文占用百分比（≥80 前端警示） */
  ctxPercent: number;
  tokens: Tokens;
  pendingApproval: PendingApproval | null;
  /** 当前运行阶段（驱动「正在思考」loading 气泡） */
  phase: AgentPhase;

  /** 发送任务；返回是否已被受理（受理后输入框可清空） */
  send: (text: string) => Promise<boolean>;
  /** 中断当前运行（Esc / 停止键） */
  interrupt: () => Promise<void>;
  /** 审批决断（允许一次 / 总是允许 / 拒绝） */
  decide: (callId: string, decision: ApprovalDecision) => Promise<void>;
  /** checkpoint 回滚 */
  rollback: (changeId: number) => Promise<void>;
  /** 切换会话后同步运行态展示 */
  resetForSession: (summary?: { ctx: number; tokens: Tokens }) => void;
  /** agent:event 统一入口（TECH_DESIGN 2.4） */
  onAgentEvent: (evt: AgentEvent) => void;
}

const EMPTY_TOKENS: Tokens = { input: 0, output: 0 };

export const useAgentStore = create<AgentState>((set, get) => ({
  runState: 'idle',
  runSessionId: null,
  todos: [],
  changes: [],
  ctxPercent: 0,
  tokens: EMPTY_TOKENS,
  pendingApproval: null,
  phase: null,

  send: async (text) => {
    const trimmed = text.trim();
    if (!trimmed) {
      toast.warning('请输入任务描述');
      return false;
    }
    const state = get().runState;
    if (state === 'running' || state === 'waiting_approval') {
      toast.warning('Agent 运行中，请先停止当前任务');
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
    const cfg = useConfigStore.getState();
    const model =
      cfg.activeModel ??
      cfg.models.find((m) => m.isDefault && m.status === 'enabled')?.name ??
      cfg.models.find((m) => m.status === 'enabled')?.name;
    if (!model) {
      toast.warning('请先在「设置 - 模型配置」中添加并启用模型');
      return false;
    }
    useSessionStore.getState().pushNode({ id: newNodeId(), kind: 'user', text: trimmed });
    set({ runState: 'running', runSessionId: sessionId, phase: 'thinking' });
    try {
      await sendTask(sessionId, trimmed, model, cfg.permMode);
      return true;
    } catch (e) {
      set({ runState: 'idle', runSessionId: null, phase: null });
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

  decide: async (callId, decision) => {
    const { runSessionId } = get();
    if (runSessionId === null) return;
    try {
      await approve(runSessionId, callId, decision);
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
    set({
      todos: [],
      changes: [],
      pendingApproval: null,
      phase: null,
      ctxPercent: summary?.ctx ?? 0,
      tokens: summary?.tokens ?? EMPTY_TOKENS,
    });
  },

  onAgentEvent: (evt) => {
    const ss = useSessionStore.getState();
    switch (evt.type) {
      case 'text_delta':
        set({ phase: 'streaming' });
        ss.appendDelta(evt.delta);
        break;
      case 'thinking_delta':
        set({ phase: 'streaming' });
        ss.appendThinkingDelta(evt.delta);
        break;
      case 'tool_start':
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
        set({ phase: 'thinking' });
        ss.updateTool(evt.callId, {
          status: evt.status,
          output: evt.output,
          note: evt.note,
          // 后端不携带 outputType，按 unified diff 头推断
          outputType: evt.output.startsWith('@@') ? 'diff' : 'code',
        });
        break;
      case 'approval_required':
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
        const map: Record<string, Exclude<ApprovalState, 'pending'>> = {
          once: 'allowed',
          always: 'always',
          auto: 'auto',
          reject: 'rejected',
          abort: 'rejected',
        };
        ss.updateApproval(evt.callId, map[evt.decision] ?? 'rejected');
        set({ pendingApproval: null, runState: 'running', phase: 'thinking' });
        if (evt.decision === 'always') {
          // 「总是允许」后端已写入 allow 规则，刷新权限规则表（PRD 验收 3）
          void useConfigStore.getState().loadPermRules();
          toast.success('已加入白名单，后续自动放行');
        }
        break;
      }
      case 'todo_update':
        set({ todos: evt.todos });
        break;
      case 'change_add': {
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
        set({ ctxPercent: evt.ctxPercent, tokens: evt.tokens });
        break;
      case 'compacted':
        ss.pushNode({
          id: newNodeId(),
          kind: 'system',
          text: `🗜 上下文已自动压缩：${evt.summary}`,
        });
        break;
      case 'run_end': {
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
        // 运行结束刷新会话列表（标题 / ctx / tokens 已变化）
        const project = useProjectStore.getState().current;
        if (project) {
          void useSessionStore
            .getState()
            .loadSessions(project.path, useSessionStore.getState().searchKw || undefined);
        }
        break;
      }
    }
  },
}));
