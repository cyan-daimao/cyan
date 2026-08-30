-- MCP 传输方式 v1：cyan_mcp_server 增加 transport / headers 列（支持远程 SSE 服务）
-- transport: stdio = 本地子进程（command 为启动命令）/ sse = 远程服务（command 为服务 URL）
-- headers:   远程服务请求头（JSON 对象文本，如 {"Authorization":"Bearer xxx"}；stdio 忽略）

ALTER TABLE cyan_mcp_server ADD COLUMN transport TEXT NOT NULL DEFAULT 'stdio';
ALTER TABLE cyan_mcp_server ADD COLUMN headers TEXT NOT NULL DEFAULT '{}';

-- 存量以 http(s) 开头的命令实为远程 SSE 服务地址，回填 transport
UPDATE cyan_mcp_server
SET transport = 'sse'
WHERE command LIKE 'http://%' OR command LIKE 'https://%';
