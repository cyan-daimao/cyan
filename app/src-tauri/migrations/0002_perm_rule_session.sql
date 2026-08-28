-- 权限规则增加会话维度：session_id NULL = 全局规则（存量数据），非 NULL = 对话级规则
-- SQLite 不支持 ALTER 修改 UNIQUE 约束，重建表

CREATE TABLE cyan_permission_rule_new (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  session_id  INTEGER REFERENCES cyan_session(id), -- 所属会话（NULL = 全局）
  tool        TEXT NOT NULL,                 -- 工具名
  pattern     TEXT NOT NULL,                 -- glob 匹配模式
  action      TEXT NOT NULL,                 -- allow/ask/deny
  sort        INTEGER NOT NULL DEFAULT 0,    -- 匹配顺序（自上而下）
  created_by  TEXT NOT NULL DEFAULT 'local',
  updated_by  TEXT NOT NULL DEFAULT 'local',
  created_at  TEXT NOT NULL,
  updated_at  TEXT NOT NULL,
  deleted_at  TEXT,
  UNIQUE (tool, pattern, session_id)
);

INSERT INTO cyan_permission_rule_new
  (id, session_id, tool, pattern, action, sort, created_by, updated_by, created_at, updated_at, deleted_at)
SELECT id, NULL, tool, pattern, action, sort, created_by, updated_by, created_at, updated_at, deleted_at
FROM cyan_permission_rule;

DROP TABLE cyan_permission_rule;
ALTER TABLE cyan_permission_rule_new RENAME TO cyan_permission_rule;

CREATE INDEX idx_perm_rule_session ON cyan_permission_rule(session_id, deleted_at);
