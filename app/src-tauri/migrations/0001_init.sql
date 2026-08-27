-- cyan 1.0.0 初始表结构（TECH_DESIGN 第 5 章，7 张表）

CREATE TABLE cyan_project (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  name        TEXT NOT NULL,                 -- 项目名
  path        TEXT NOT NULL,                 -- 绝对路径（canonicalize 后）
  last_opened_at TEXT,                       -- 最近打开时间
  created_by  TEXT NOT NULL DEFAULT 'local', -- 创建人
  updated_by  TEXT NOT NULL DEFAULT 'local', -- 更新人
  created_at  TEXT NOT NULL,                 -- 创建时间
  updated_at  TEXT NOT NULL,                 -- 更新时间
  deleted_at  TEXT,                          -- 删除时间（软删）
  UNIQUE (path)
);

CREATE TABLE cyan_session (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  project_id  INTEGER NOT NULL REFERENCES cyan_project(id),
  title       TEXT NOT NULL DEFAULT '新会话', -- 会话标题（首条任务截断生成）
  ctx_percent INTEGER NOT NULL DEFAULT 0,    -- 上下文占用百分比
  input_tokens  INTEGER NOT NULL DEFAULT 0,  -- 累计输入 token
  output_tokens INTEGER NOT NULL DEFAULT 0,  -- 累计输出 token
  created_by  TEXT NOT NULL DEFAULT 'local',
  updated_by  TEXT NOT NULL DEFAULT 'local',
  created_at  TEXT NOT NULL,
  updated_at  TEXT NOT NULL,
  deleted_at  TEXT
);
CREATE INDEX idx_session_project ON cyan_session(project_id, deleted_at, updated_at DESC);

CREATE TABLE cyan_message (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  session_id  INTEGER NOT NULL REFERENCES cyan_session(id),
  seq         INTEGER NOT NULL,              -- 会话内序号
  kind        TEXT NOT NULL,                 -- user/assistant/tool/approval/system
  payload     TEXT NOT NULL,                 -- JSON：文本或工具卡/审批卡结构
  created_by  TEXT NOT NULL DEFAULT 'local',
  updated_by  TEXT NOT NULL DEFAULT 'local',
  created_at  TEXT NOT NULL,
  updated_at  TEXT NOT NULL,
  deleted_at  TEXT,
  UNIQUE (session_id, seq)
);

CREATE TABLE cyan_model_config (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  name        TEXT NOT NULL,                 -- 模型名，唯一
  provider    TEXT NOT NULL,                 -- Provider
  base_url    TEXT NOT NULL,                 -- Base URL
  api_key     TEXT NOT NULL,                 -- API Key 引用串（keychain://cyan/model/<name>）
  context_window INTEGER NOT NULL,           -- 上下文窗口
  is_default  INTEGER NOT NULL DEFAULT 0,    -- 是否默认（应用层保证唯一）
  status      TEXT NOT NULL DEFAULT 'enabled', -- enabled/disabled
  created_by  TEXT NOT NULL DEFAULT 'local',
  updated_by  TEXT NOT NULL DEFAULT 'local',
  created_at  TEXT NOT NULL,
  updated_at  TEXT NOT NULL,
  deleted_at  TEXT,
  UNIQUE (name)
);

CREATE TABLE cyan_mcp_server (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  name        TEXT NOT NULL,                 -- 服务器名，唯一
  command     TEXT NOT NULL,                 -- 启动命令
  status      TEXT NOT NULL DEFAULT 'disabled', -- connected/error/disabled
  tools       INTEGER NOT NULL DEFAULT 0,    -- 握手发现的工具数
  last_error  TEXT,                          -- 最近失败原因
  created_by  TEXT NOT NULL DEFAULT 'local',
  updated_by  TEXT NOT NULL DEFAULT 'local',
  created_at  TEXT NOT NULL,
  updated_at  TEXT NOT NULL,
  deleted_at  TEXT,
  UNIQUE (name)
);

CREATE TABLE cyan_permission_rule (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  tool        TEXT NOT NULL,                 -- 工具名
  pattern     TEXT NOT NULL,                 -- glob 匹配模式
  action      TEXT NOT NULL,                 -- allow/ask/deny
  sort        INTEGER NOT NULL DEFAULT 0,    -- 匹配顺序（自上而下）
  created_by  TEXT NOT NULL DEFAULT 'local',
  updated_by  TEXT NOT NULL DEFAULT 'local',
  created_at  TEXT NOT NULL,
  updated_at  TEXT NOT NULL,
  deleted_at  TEXT,
  UNIQUE (tool, pattern)
);

CREATE TABLE cyan_checkpoint (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  session_id  INTEGER NOT NULL REFERENCES cyan_session(id),
  file_path   TEXT NOT NULL,                 -- 变更文件（相对项目）
  git_ref     TEXT NOT NULL,                 -- checkpoint 对应的 git blob 引用
  add_lines   INTEGER NOT NULL DEFAULT 0,    -- 新增行数
  del_lines   INTEGER NOT NULL DEFAULT 0,    -- 删除行数
  rolled_back INTEGER NOT NULL DEFAULT 0,    -- 是否已回滚
  created_by  TEXT NOT NULL DEFAULT 'local',
  updated_by  TEXT NOT NULL DEFAULT 'local',
  created_at  TEXT NOT NULL,
  updated_at  TEXT NOT NULL,
  deleted_at  TEXT
);
