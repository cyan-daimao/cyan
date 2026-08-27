import { listen } from '@tauri-apps/api/event';
import type { UnlistenFn } from '@tauri-apps/api/event';
import { call } from './invoke';
import type { AgentEvent, ApprovalDecision, PermMode } from '../types';

/** Agent 运行时命令与事件订阅（参数与 src-tauri agent_command.rs / agent_dto.rs 一致） */

export const sendTask = (sessionId: number, text: string, model: string, permMode: PermMode) =>
  call<void>('send_task', { request: { sessionId, text, model, permMode } });

export const interruptRun = (sessionId: number) =>
  call<void>('interrupt_run', { request: { sessionId } });

export const approve = (sessionId: number, callId: string, decision: ApprovalDecision) =>
  call<void>('approve', { request: { sessionId, callId, decision } });

export const rollbackChange = (sessionId: number, changeId: number) =>
  call<void>('rollback_change', { request: { sessionId, changeId } });

/** 订阅 agent:event 单通道事件，返回解绑函数 */
export const listenAgentEvents = (handler: (evt: AgentEvent) => void): Promise<UnlistenFn> =>
  listen<AgentEvent>('agent:event', (e) => handler(e.payload));
