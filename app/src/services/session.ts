import { call } from './invoke';
import type { ProjectTokenUsageDTO, SessionDTO, SessionSummaryDTO } from '../types';

/** 会话相关命令（参数与 src-tauri session_command.rs / session_dto.rs 一致） */

export const listSessions = (projectPath: string, keyword?: string) =>
  call<SessionSummaryDTO[]>('list_sessions', { request: { projectPath, keyword } });

export const getSession = (sessionId: number) =>
  call<SessionDTO>('get_session', { request: { sessionId } });

export const createSession = (projectPath: string) =>
  call<SessionDTO>('create_session', { request: { projectPath } });

export const deleteSession = (sessionId: number) =>
  call<void>('delete_session', { request: { sessionId } });

/** 会话级模型偏好；model 传空串 = 清除偏好（跟随全局） */
export const setSessionModel = (sessionId: number, model: string) =>
  call<void>('set_session_model', { request: { sessionId, model } });

export const projectTokenUsage = (projectPath: string) =>
  call<ProjectTokenUsageDTO>('project_token_usage', { request: { projectPath } });

/* ---- 回收站 ---- */

/** 已删除会话列表（调用风格与 list_sessions 一致） */
export const listDeletedSessions = () =>
  call<SessionDTO[]>('list_deleted_sessions', { request: {} });

/** 恢复已删除会话 */
export const restoreSession = (id: number) =>
  call<void>('restore_session', { request: { id } });

/** 清空回收站（硬删），返回清理行数 */
export const purgeRecycleBin = () => call<number>('purge_recycle_bin');

/**
 * 行内编辑消息：更新 payload 文本 + 物理删除其后所有消息，返回完整会话。
 * （编辑即截断重发语义）
 */
export const editMessage = (id: number, text: string) =>
  call<SessionDTO>('edit_message', { request: { id, text } });
