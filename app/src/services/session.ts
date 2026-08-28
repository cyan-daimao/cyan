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

export const projectTokenUsage = (projectPath: string) =>
  call<ProjectTokenUsageDTO>('project_token_usage', { request: { projectPath } });
