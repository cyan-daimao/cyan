-- 权限规则三级作用域：全局（双 NULL）/ 本项目（project_id）/ 本会话（session_id）
-- SQLite 不支持 ALTER 修改 UNIQUE 约束，重建表

CREATE TABLE cyan_permission_rule_new (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  project_id  INTEGER REFERENCES cyan_project(id),  -- 所属项目（会话级规则同时回填）
  session_id  INTEGER REFERENCES cyan_session(id),  -- 所属会话（与 project_id 双 NULL = 全局）
  tool        TEXT NOT NULL,                 -- 工具名
  pattern     TEXT NOT NULL,                 -- glob 匹配模式
  action      TEXT NOT NULL,                 -- allow/ask/deny
  sort        INTEGER NOT NULL DEFAULT 0,    -- 匹配顺序（自上而下）
  created_by  TEXT NOT NULL DEFAULT 'local',
  updated_by  TEXT NOT NULL DEFAULT 'local',
  created_at  TEXT NOT NULL,
  updated_at  TEXT NOT NULL,
  deleted_at  TEXT,
  UNIQUE (tool, pattern, project_id, session_id)
);

INSERT INTO cyan_permission_rule_new
  (id, project_id, session_id, tool, pattern, action, sort, created_by, updated_by, created_at, updated_at, deleted_at)
SELECT
  r.id,
  -- 存量会话级规则回填 project_id；全局规则保持 NULL
  (SELECT s.project_id FROM cyan_session s WHERE s.id = r.session_id),
  r.session_id,
  r.tool, r.pattern, r.action, r.sort, r.created_by, r.updated_by, r.created_at, r.updated_at, r.deleted_at
FROM cyan_permission_rule r;

DROP TABLE cyan_permission_rule;
ALTER TABLE cyan_permission_rule_new RENAME TO cyan_permission_rule;

CREATE INDEX idx_perm_rule_scope ON cyan_permission_rule(session_id, project_id, deleted_at);
