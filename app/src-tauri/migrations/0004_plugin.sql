-- 插件 v1：cyan_plugin 表 + MCP/权限规则表增加 plugin_origin 溯源列（PLUGIN_DESIGN 第 3 节）
-- plugin_origin NULL = 用户自建；非 NULL = 插件名（卸载时按此反向清理）

CREATE TABLE cyan_plugin (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  name        TEXT NOT NULL,                 -- 插件名（唯一，即包目录名）
  version     TEXT NOT NULL,                 -- 版本
  author      TEXT NOT NULL DEFAULT '',      -- 作者
  description TEXT NOT NULL DEFAULT '',      -- 描述
  status      TEXT NOT NULL DEFAULT 'enabled', -- enabled/disabled
  skill_count INTEGER NOT NULL DEFAULT 0,    -- 携带技能数
  mcp_count   INTEGER NOT NULL DEFAULT 0,    -- 携带 MCP 服务器数
  rule_count  INTEGER NOT NULL DEFAULT 0,    -- 携带权限规则数
  created_by  TEXT NOT NULL DEFAULT 'local',
  updated_by  TEXT NOT NULL DEFAULT 'local',
  created_at  TEXT NOT NULL,                 -- 安装时间
  updated_at  TEXT NOT NULL,
  deleted_at  TEXT,
  UNIQUE (name)
);

-- ALTER ADD COLUMN 不触及既有 UNIQUE 约束，SQLite 直接支持
ALTER TABLE cyan_mcp_server ADD COLUMN plugin_origin TEXT;
ALTER TABLE cyan_permission_rule ADD COLUMN plugin_origin TEXT;
