import { call } from './invoke';
import type {
  McpMarketItemDTO,
  McpServerDTO,
  ModelDTO,
  PermAction,
  PermRuleDTO,
  RuleScope,
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

/** 保存（含握手验证）；id 编辑时携带，新增传 undefined；sse 需传服务 URL 与请求头（JSON 对象文本） */
export const saveMcpServer = (
  id: number | undefined,
  name: string,
  command: string,
  transport: 'stdio' | 'sse' = 'stdio',
  headers = '{}',
) => call<McpServerDTO>('save_mcp_server', { request: { id, name, command, transport, headers } });

export const toggleMcpServer = (id: number, enable: boolean) =>
  call<McpServerDTO>('toggle_mcp_server', { request: { id, enable } });

export const deleteMcpServer = (id: number) =>
  call<void>('delete_mcp_server', { request: { id } });

/** MCP 市场：keyword 空返回精选列表；有关键字时精选在前 + registry 结果在后 */
export const searchMcpMarket = (keyword: string) =>
  call<McpMarketItemDTO[]>('search_mcp_market', { request: { keyword } });

/* ---- 权限规则（三级作用域：全局 / 本项目 / 本会话） ---- */

/** 全局规则（设置页管理） */
export const listGlobalPermRules = () => call<PermRuleDTO[]>('list_global_perm_rules');

/** 会话可见规则（全局 + 项目 + 会话） */
export const listVisiblePermRules = (sessionId: number, projectId: number) =>
  call<PermRuleDTO[]>('list_visible_perm_rules', { request: { sessionId, projectId } });

/** id 编辑时携带；新建必须带 scope（project/session 需对应 id）；sort 为必填匹配顺序 */
export const savePermRule = (
  id: number | undefined,
  scope: RuleScope | undefined,
  projectId: number | undefined,
  sessionId: number | undefined,
  tool: string,
  pattern: string,
  action: PermAction,
  sort: number,
) =>
  call<PermRuleDTO>('save_perm_rule', {
    request: { id, scope, projectId, sessionId, tool, pattern, action, sort },
  });

export const deletePermRule = (id: number) =>
  call<void>('delete_perm_rule', { request: { id } });
