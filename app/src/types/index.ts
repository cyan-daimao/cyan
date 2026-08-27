/**
 * 与 Rust adapter DTO 一一对应的前端类型定义。
 * 权威来源：src-tauri/src/adapter/dto/*.rs 与 adapter/dto/agent_dto.rs 的 AgentEventDTO
 * （serde camelCase；所有数据库 id 为 i64 → 前端 number；callId 为 String → string）。
 */

/** token 用量（TokenStatDTO / TokenUsageDTO） */
export interface Tokens {
  input: number;
  output: number;
}

/** 权限模式 */
export type PermMode = 'plan' | 'ask' | 'auto';
/** 会话运行状态机（PRD 8.1） */
export type RunState = 'idle' | 'running' | 'waiting_approval' | 'error';
/** 工具调用状态（事件流仅 ok/error；denied 为前端展示保留态） */
export type ToolStatus = 'running' | 'ok' | 'error' | 'denied';
/** 审批卡状态：pending → allowed / always / auto / rejected */
export type ApprovalState = 'pending' | 'allowed' | 'always' | 'auto' | 'rejected';
/** 审批决断（approve 命令入参，domain permission.rs as_str） */
export type ApprovalDecision = 'once' | 'always' | 'reject';
/** TODO 状态（domain TodoItem.status：pending / in_progress / done） */
export type TodoState = 'pending' | 'in_progress' | 'done';
/** 模型状态 */
export type ModelStatus = 'enabled' | 'disabled';
/** MCP 服务器状态 */
export type McpStatus = 'connected' | 'error' | 'disabled';
/** 权限规则动作 */
export type PermAction = 'allow' | 'ask' | 'deny';
/** 项目脚手架模板（domain project.rs：empty / rust / node） */
export type ProjectTemplate = 'empty' | 'rust' | 'node';

/** 会话摘要 DTO */
export interface SessionSummaryDTO {
  id: number;
  title: string;
  /** 时间分组（今天/昨天/本周/更早） */
  group: string;
  /** 上下文占用百分比 */
  ctx: number;
  tokens: Tokens;
  createdAt: string;
  updatedAt: string;
}

/** 消息种类 */
export type MessageKind = 'user' | 'assistant' | 'tool' | 'approval' | 'system';

/**
 * 消息 DTO。后端 payload 为 serde_json::Value（已解析的 JSON 对象）：
 * - user/assistant：`{"text": ...}`（assistant 可能附带 thinking 与 toolCalls）
 * - tool：`{"callId","tool","arg","status","output","note"?}`
 * - approval：`{"callId","tool","arg","decision"}`（decision: once/always/reject/auto/abort）
 * - system：`{"text": ...}`
 */
export interface MessageDTO {
  id: number;
  seq: number;
  kind: MessageKind;
  payload: Record<string, unknown>;
  createdAt: string;
}

/** 会话详情 DTO（含全部消息） */
export interface SessionDTO {
  id: number;
  projectId: number;
  title: string;
  ctx: number;
  tokens: Tokens;
  messages: MessageDTO[];
  createdAt: string;
  updatedAt: string;
}

/** 前端渲染用消息节点（由 MessageDTO.payload / 事件流构造；id 为本地生成） */
export type ChatNode =
  | { id: string; kind: 'user'; text: string }
  | { id: string; kind: 'assistant'; text: string; thinking?: string; streaming?: boolean }
  | {
      id: string;
      kind: 'tool';
      callId: string;
      tool: string;
      arg: string;
      status: ToolStatus;
      outputType?: 'code' | 'diff' | 'text';
      output?: string;
      note?: string;
    }
  | {
      id: string;
      kind: 'approval';
      callId: string;
      tool: string;
      arg: string;
      reason: string;
      state: ApprovalState;
    }
  | { id: string; kind: 'system'; text: string };

/** TODO 项 DTO（agent_dto.rs TodoDTO） */
export interface TodoDTO {
  id: number;
  content: string;
  status: TodoState;
}

/** 文件变更（checkpoint）DTO（agent_dto.rs ChangeDTO） */
export interface ChangeDTO {
  changeId: number;
  filePath: string;
  addLines: number;
  delLines: number;
  rolledBack: boolean;
}

/**
 * 抽屉展示用变更：后端 ChangeDTO + 前端从对应 Edit/Write 工具卡捕获的 diff 快照
 * （验收 11：查看 diff 弹窗内容与原 Edit 卡一致）。
 */
export interface ChangeView extends ChangeDTO {
  diff: string;
}

/** 项目 DTO */
export interface ProjectDTO {
  id: number;
  name: string;
  path: string;
  /** 最近打开时间（YYYY-MM-DD HH:MM:SS） */
  lastOpenedAt?: string;
}

/** 模型 DTO（API Key 已脱敏为 maskedKey） */
export interface ModelDTO {
  id: number;
  name: string;
  provider: string;
  baseUrl: string;
  maskedKey: string;
  contextWindow: number;
  isDefault: boolean;
  status: ModelStatus;
}

/** 保存模型请求：id 编辑时携带；apiKey 空/缺省表示不修改 */
export interface SaveModelRequest {
  id?: number;
  name: string;
  provider: string;
  baseUrl: string;
  apiKey?: string;
  contextWindow: number;
  enabled: boolean;
}

/** MCP 服务器 DTO */
export interface McpServerDTO {
  id: number;
  name: string;
  command: string;
  status: McpStatus;
  /** 握手发现的工具数；非 connected 前端展示 — */
  tools: number;
  lastError?: string;
}

/** 权限规则 DTO */
export interface PermRuleDTO {
  id: number;
  tool: string;
  pattern: string;
  action: PermAction;
  /** 匹配顺序（自上而下） */
  sort: number;
}

/** 文件树节点 DTO（file_dto.rs：name / path / isDir / children） */
export interface FileNodeDTO {
  name: string;
  /** 相对项目根路径 */
  path: string;
  isDir: boolean;
  children: FileNodeDTO[];
}

/** 文件预览 DTO（≤ 64KB，超出 truncated=true） */
export interface FilePreviewDTO {
  content: string;
  truncated: boolean;
}

/** approval_resolved 事件的决断取值（domain：once/always/reject/auto/abort） */
export type ResolvedDecision = 'once' | 'always' | 'reject' | 'auto' | 'abort';

/**
 * agent:event 单通道事件载荷。
 * 后端 AgentEventDTO 采用 `#[serde(tag="type", rename_all="snake_case", rename_all_fields="camelCase")]`
 * 扁平序列化：type 为 snake_case 判别，其余字段 camelCase。
 */
export type AgentEvent =
  | { type: 'text_delta'; sessionId: number; delta: string }
  | { type: 'thinking_delta'; sessionId: number; delta: string }
  | { type: 'tool_start'; sessionId: number; callId: string; tool: string; arg: string }
  | {
      type: 'tool_end';
      sessionId: number;
      callId: string;
      status: 'ok' | 'error';
      output: string;
      note?: string;
    }
  | {
      type: 'approval_required';
      sessionId: number;
      callId: string;
      tool: string;
      arg: string;
      reason: string;
    }
  | {
      type: 'approval_resolved';
      sessionId: number;
      callId: string;
      decision: ResolvedDecision;
    }
  | { type: 'todo_update'; sessionId: number; todos: TodoDTO[] }
  | { type: 'change_add'; sessionId: number; change: ChangeDTO }
  | { type: 'ctx_update'; sessionId: number; ctxPercent: number; tokens: Tokens }
  | { type: 'compacted'; sessionId: number; summary: string }
  | {
      type: 'run_end';
      sessionId: number;
      result: 'done' | 'aborted' | 'error';
      /** result=error 时的错误信息 */
      message?: string;
      usage: Tokens;
    };
