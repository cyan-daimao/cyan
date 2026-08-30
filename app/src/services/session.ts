import { call } from './invoke';
import type {
  ProjectTokenUsageDTO,
  RecycleBinDTO,
  RecycleKind,
  SessionDTO,
  SessionSummaryDTO,
} from '../types';

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

/** 回收站全对象列表（会话/项目/模型/MCP/插件/规则/技能） */
export const listRecycleBin = () => call<RecycleBinDTO>('list_recycle_bin');

/** 恢复回收站条目；恢复项目会带回随删会话，恢复会话在项目已删时连带恢复项目 */
export const restoreRecycleItem = (kind: RecycleKind, id: number | string) =>
  call<void>('restore_recycle_item', { request: { kind, id } });

/**
 * 行内编辑消息：更新 payload 文本 + 物理删除其后所有消息，返回完整会话。
 * （编辑即截断重发语义）
 */
export const editMessage = (id: number, text: string) =>
  call<SessionDTO>('edit_message', { request: { id, text } });

/** 重命名会话标题（trim 后 1..=80 字符；幂等） */
export const renameSession = (id: number, title: string) =>
  call<void>('rename_session', { request: { id, title } });

/** 清空会话上下文（/clear）：硬删全部消息与 checkpoint，统计归零；返回清除的消息数 */
export const clearSession = (sessionId: number) =>
  call<number>('clear_session', { request: { sessionId } });
