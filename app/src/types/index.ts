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

/** 项目级 token 用量 DTO（ProjectTokenUsageDTO） */
export interface ProjectTokenUsageDTO {
  /** 累计输入 token */
  inputTokens: number;
  /** 累计输出 token */
  outputTokens: number;
  /** 会话数 */
  sessionCount: number;
}

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
  /** 删除时间（回收站用；未删除为 null） */
  deletedAt?: string | null;
  /** 所属项目名称（回收站列表携带；常规打开为空串） */
  projectName?: string;
  /** 所属项目路径（同上） */
  projectPath?: string;
  /** 会话级模型偏好（null/缺省 = 跟随全局 activeModel） */
  preferredModel?: string | null;
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
      /** 工具执行中实时输出（tool_delta 内存态缓冲，上限 50KB 截头；tool_end 后清空） */
      liveOutput?: string;
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
  /** 删除时间（回收站用；未删除为 null） */
  deletedAt?: string | null;
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
  /** 删除时间（回收站用；未删除为 null） */
  deletedAt?: string | null;
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
  /** 删除时间（回收站用；未删除为 null） */
  deletedAt?: string | null;
}

/** 权限规则作用域 */
export type RuleScope = 'global' | 'project' | 'session';

/** 权限规则 DTO */
export interface PermRuleDTO {
  id: number;
  /** 作用域：global 全局 / project 本项目 / session 本会话 */
  scope: RuleScope;
  /** 所属项目 id（非项目级为 null） */
  projectId: number | null;
  /** 所属会话 id（非会话级为 null） */
  sessionId: number | null;
  tool: string;
  pattern: string;
  action: PermAction;
  /** 匹配顺序（自上而下） */
  sort: number;
  /** 删除时间（回收站用；未删除为 null） */
  deletedAt?: string | null;
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
  | { type: 'tool_delta'; sessionId: number; callId: string; delta: string }
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
  /** 单窗口工具轮次跑满、任务未完成，自动续跑（round 从 1 开始） */
  | { type: 'run_continued'; sessionId: number; round: number }
  | {
      type: 'run_end';
      sessionId: number;
      result: 'done' | 'aborted' | 'error';
      /** result=error 时的错误信息 */
      message?: string;
      usage: Tokens;
    };

/* ============================================================
 * 技能（Skill）v1 — PLUGIN_DESIGN 第 2 节
 * ============================================================ */

/** 技能作用域：全局 ~/.cyan/skills / 项目 <项目根>/.cyan/skills / 插件携带 */
export type SkillScope = 'global' | 'project';

/** 技能来源（plugin 来源的技能由插件携带，只读） */
export type SkillSource = SkillScope | 'plugin';

/** 技能 DTO（文件名即技能 id） */
export interface SkillDTO {
  /** 文件名（kebab-case，不含扩展名） */
  id: string;
  name: string;
  description: string;
  enabled: boolean;
  source: SkillSource;
  /** 插件来源时的插件名（其他来源为 null） */
  pluginName: string | null;
  /** 技能市场来源仓库（owner/repo；手动创建/插件携带为 null） */
  marketRepo: string | null;
  /** 正文 prompt 模板，支持 $ARGUMENTS 占位符 */
  content: string;
}

/** 保存技能请求：scope=project 时必须携带 projectPath */
export interface SaveSkillRequest {
  scope: SkillScope;
  fileName: string;
  name: string;
  description: string;
  enabled: boolean;
  content: string;
  projectPath?: string;
}

/* ============================================================
 * 插件（Plugin）v1 — PLUGIN_DESIGN 第 3 节（声明式能力包）
 * ============================================================ */

/** 插件状态 */
export type PluginStatus = 'enabled' | 'disabled';

/** 插件 DTO */
export interface PluginDTO {
  id: number;
  name: string;
  version: string;
  author: string;
  description: string;
  status: PluginStatus;
  /** 内容物计数：技能 / MCP 服务器 / 权限规则 */
  skillCount: number;
  mcpCount: number;
  ruleCount: number;
  /** 安装时间（YYYY-MM-DD HH:MM:SS） */
  installedAt: string;
  /** sidecar 后端进程是否在运行 */
  backendRunning: boolean;
  /** sidecar 后端端口（cyan 分配；无 backend 声明为 null） */
  backendPort: number | null;
  /** 删除时间（回收站用；未删除为 null） */
  deletedAt?: string | null;
}

/* ============================================================
 * 插件市场（PLUGIN_DESIGN 3.2 市场 = git 仓库协议的 GitHub 搜索形态）
 * ============================================================ */

/** 插件市场条目 DTO（GitHub 仓库搜索结果） */
export interface MarketItemDTO {
  /** owner/repo */
  fullName: string;
  description: string | null;
  stars: number;
  author: string;
  url: string;
}

/* ============================================================
 * MCP 市场（精选 featured + 官方 registry 搜索）
 * ============================================================ */

/** MCP 市场条目 DTO */
export interface McpMarketItemDTO {
  /** registry 全名或精选短名 */
  name: string;
  /** 展示名 */
  title: string;
  description: string;
  /** featured 为 '' 或 'latest' */
  version: string;
  /** 启动命令；null = 远程服务暂不支持安装 */
  command: string | null;
  source: 'featured' | 'registry';
  homepage: string | null;
}

/* ============================================================
 * 回收站（全对象）
 * ============================================================ */

/** 回收站对象种类 */
export type RecycleKind =
  | 'session'
  | 'project'
  | 'model'
  | 'mcp'
  | 'plugin'
  | 'permRule'
  | 'skill';

/** 回收站技能条目（后端可选提供） */
export interface RecycleSkillDTO {
  id: string;
  name: string;
  scope: SkillScope;
  fileName: string;
  deletedAt?: string | null;
}

/** 回收站全对象列表 DTO */
export interface RecycleBinDTO {
  sessions: SessionDTO[];
  projects: ProjectDTO[];
  models: ModelDTO[];
  mcpServers: McpServerDTO[];
  plugins: PluginDTO[];
  permRules: PermRuleDTO[];
  skills?: RecycleSkillDTO[];
}
