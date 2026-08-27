import { call } from './invoke';
import type {
  McpServerDTO,
  ModelDTO,
  PermAction,
  PermRuleDTO,
  SaveModelRequest,
} from '../types';

/** 配置相关命令：模型 / MCP / 权限规则（与 src-tauri config_command.rs / config_dto.rs 一致） */

/* ---- 模型 ---- */

export const listModels = () => call<ModelDTO[]>('list_models');

export const saveModel = (request: SaveModelRequest) =>
  call<ModelDTO>('save_model', { request });

export const deleteModel = (id: number) =>
  call<void>('delete_model', { request: { id } });

export const setDefaultModel = (id: number) =>
  call<void>('set_default_model', { request: { id } });

/* ---- MCP 服务器 ---- */

export const listMcpServers = () => call<McpServerDTO[]>('list_mcp_servers');

/** 保存（含握手验证）；id 编辑时携带，新增传 undefined */
export const saveMcpServer = (id: number | undefined, name: string, command: string) =>
  call<McpServerDTO>('save_mcp_server', { request: { id, name, command } });

export const toggleMcpServer = (id: number, enable: boolean) =>
  call<McpServerDTO>('toggle_mcp_server', { request: { id, enable } });

export const deleteMcpServer = (id: number) =>
  call<void>('delete_mcp_server', { request: { id } });

/* ---- 权限规则 ---- */

export const listPermRules = () => call<PermRuleDTO[]>('list_perm_rules');

/** id 编辑时携带；sort 为必填匹配顺序 */
export const savePermRule = (
  id: number | undefined,
  tool: string,
  pattern: string,
  action: PermAction,
  sort: number,
) => call<PermRuleDTO>('save_perm_rule', { request: { id, tool, pattern, action, sort } });

export const deletePermRule = (id: number) =>
  call<void>('delete_perm_rule', { request: { id } });
