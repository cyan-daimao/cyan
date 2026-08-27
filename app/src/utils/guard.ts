import { useAgentStore } from '../stores/agentStore';
import { toast } from './feedback';

/** 运行中（含等待审批）判定：PRD 8.4 禁止操作的统一守卫 */
export function isBusy(): boolean {
  const s = useAgentStore.getState().runState;
  return s === 'running' || s === 'waiting_approval';
}

/**
 * 运行中拦截守卫：返回 true 表示已拦截（并弹出黄点 Toast）。
 * 用法：`if (guardBusy('切换会话')) return;`
 */
export function guardBusy(action: string): boolean {
  if (isBusy()) {
    toast.warning(`Agent 运行中，请先停止当前任务再${action}`);
    return true;
  }
  return false;
}
