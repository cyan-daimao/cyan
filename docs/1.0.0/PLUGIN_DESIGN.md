# cyan 技能与插件体系技术方案

> 版本：v1.0 · 状态：技能 v1 已落地，插件分阶段实施
> 分层约定遵循 DDBD：adapter → application → domain → infra

## 1. 目标与定位

为 cyan 提供可扩展能力体系，对标 Claude Code 的 skills/plugins 与 VS Code 扩展模型：

- **技能（Skill）**：Markdown 定义的工作流模板，文件即配置，不写代码
- **插件（Plugin）**：声明式能力包（v1）→ 带 UI 的沙箱前端（v2）→ 带 sidecar 后端的完整程序（v3）

## 2. 技能（Skill）v1 — 已落地

### 2.1 技能文件格式

一个技能 = 一个 Markdown 文件，文件名即技能 id（kebab-case，不含扩展名）：

```markdown
---
name: 周报
description: 汇总本周 git 提交生成周报
enabled: true        # 可选，缺省 true
---
正文 prompt 模板，支持 $ARGUMENTS 占位符（用户输入的参数原样替换）。
```

### 2.2 目录与作用域

| 作用域 | 目录 | 生效范围 |
| --- | --- | --- |
| 全局 | `~/.cyan/skills/*.md` | 所有项目 |
| 项目 | `<项目根>/.cyan/skills/*.md` | 当前项目 |

同名技能项目级覆盖全局。

### 2.3 触发方式

输入框以 `/` 开头时弹出技能补全列表（名称/描述/来源），选中后技能正文展开到输入框，`$ARGUMENTS` 由用户继续填写。展开后的内容就是普通 prompt，走现有 Agent 管道，运行时零侵入。

### 2.4 后端对象与命令

- domain：`Skill { id, name, description, enabled, source, content, market_repo }`（充血：`expand(args)` 做参数替换）
- application：`SkillService`（list/save/delete，按作用域落盘；v1.2 起含技能市场 search/install）
- infra：`infra/fs/skill.rs` 目录扫描 + frontmatter 解析（`---` 包围的 key: value 行）
- adapter 命令：`list_skills` / `save_skill` / `delete_skill` / `search_skill_market` / `install_skill_from_github`

### 2.5 面板管理

能力面板「技能」Tab：列表（名称/描述/来源/启用状态）+ 新增/编辑（文件名、名称、描述、正文）+ 删除。

### 2.6 技能市场（v1.2 已落地，v1.3 扩展）

- **仓库约定**：GitHub 仓库打 `cyan-skill` topic；zip 解压后顶层 `*.md`（排除 `README.md`）与 `skills/*.md` 均视为技能文件，文件名即技能 id，格式同 2.1（frontmatter + 正文）。v1.3 起兼容 Claude 目录式布局：一层子目录 `*/SKILL.md`（大小写不敏感）也收录，技能 id 取目录名——GitHub 上存量 Claude skills 仓库可直接安装。
- **搜索**：`search_skill_market` 走 GitHub Search API，v1.3 起并发搜 `topic:cyan-skill` + `topic:claude-skill` 两路，合并去重按 stars 排序；支持粘贴 `owner/repo` 或 GitHub URL 直接安装。
- **安装**：`install_skill_from_github` 下载 codeload zip → 收集技能 md → 逐个校验 id（与全局同名冲突则整体回滚报错）→ 写入 `~/.cyan/skills/` 并在 frontmatter 注入 `market: owner/repo` 溯源。
- **溯源**：`Skill.market_repo`（frontmatter `market` 键），面板「已安装」判定与来源展示依赖该字段；删除技能即清除溯源。

## 3. 插件 v1 — 声明式能力包

### 3.1 包结构

```
my-plugin.zip（或目录）
├── manifest.json
├── skills/*.md          # 可选：技能集合
├── mcp.json             # 可选：MCP 服务器声明
└── rules.json           # 可选：权限规则预设
```

### 3.2 manifest.json

```json
{
  "name": "my-plugin",
  "version": "0.1.0",
  "author": "someone",
  "description": "…",
  "cyan_min_version": "0.1.0",
  "permissions": ["skills", "mcp", "rules"]
}
```

### 3.3 安装与生命周期

- 安装 = 解包到 `~/.cyan/plugins/<name>/`，注册内容物：skills 挂载进技能扫描、MCP 声明写入 `cyan_mcp_server`、rules 写入全局权限规则
- 启用/禁用：数据库 `cyan_plugin` 表记录状态；禁用时其内容物整体摘除
- 卸载：按 `plugin_origin` 字段反向清理（MCP/规则表需加该字段）
- 分发：本地 zip/文件夹安装；插件市场 = GitHub（约定插件仓库打 `cyan-plugin` topic，仓库根放 manifest.json）
- **市场搜索**（已落地）：`search_marketplace` 走 GitHub Search API（`topic:cyan-plugin` + 关键词，按 stars 排序，未认证限流 10 次/分钟）；支持粘贴 `owner/repo` 或 GitHub URL 直接安装
- **一键安装**（已落地）：`install_plugin_from_github` 下载 codeload zip（`zip/HEAD`）后复用 zip 安装管道，顶层包裹目录自动剥离

## 4. 插件 v2 — 沙箱 UI 插件

### 4.1 架构

插件携带静态前端 bundle（`ui/index.html` + 资源），宿主以 `<iframe sandbox="allow-scripts">` 加载，通过 `postMessage` 与宿主通信。插件无权直接访问 DOM/系统/Tauri API。

Tauri 侧：`asset` protocol scope 增加 `~/.cyan/plugins/**`，iframe src 用 asset URL。

### 4.2 Bridge 消息协议（宿主 ←→ 插件）

请求/响应模型，`requestId` 关联：

```json
// 插件 → 宿主
{ "source": "cyan-plugin", "type": "request", "requestId": "1", "api": "fs.read", "args": { "path": "README.md" } }
// 宿主 → 插件
{ "source": "cyan-host", "type": "response", "requestId": "1", "ok": true, "data": "…" }
```

宿主 bridge 职责：校验 `event.origin` 与插件身份绑定、按 manifest `permissions` 逐项拦截、API 版本协商（`cyan_api_version`）。

### 4.3 v2 API 白名单（初版）

| API | 权限 | 说明 |
| --- | --- | --- |
| `fs.read` | `fs:read` | 读当前项目内文本文件（≤64KB） |
| `agent.run` | `agent:run` | 发起一次 Agent 任务（当前会话） |
| `notify` | — | toast 通知 |
| `storage.get/set` | — | 插件私有 KV（`~/.cyan/plugins/<name>/storage.json`） |
| `theme.info` | — | 当前主题色/背景主题 |

### 4.4 UI 挂载点

插件声明 `contributes.panels`（侧栏面板 / 对话框页面），宿主提供挂载容器与布局约束。

## 5. 插件 v3 — sidecar 后端

- manifest 增加 `server.command`：宿主拉起子进程，通信走 stdio JSON-RPC 或 loopback HTTP + 一次性 token
- 复用 `infra/mcp` 的子进程管理；与 MCP 的差异：sidecar 不只暴露工具给 Agent，还可给插件 UI 供数据
- 安全边界：子进程无法真沙箱 → 配合 Seatbelt 沙箱（独立安全项）落地；此前靠「安装时权限明示 + 用户信任」

## 6. MCP 市场（v1.3 已落地）

- **精选区**：内置 8 个知名开源 MCP server（Context7 / Playwright / Chrome DevTools / Filesystem / GitHub / Memory / Fetch / Time），硬编码在 `infra/mcp_registry.rs::featured_servers`，无需对方打任何 topic。
- **registry 搜索**：`search_mcp_market` 走 MCP 官方 registry（`GET registry.modelcontextprotocol.io/v0/servers?search=`），按 `isLatest` 过滤同名多版本；npm 包映射 `npx -y <包名>`、pypi 映射 `uvx <包名>`，仅 stdio transport 可装，远程服务（command 为空）前端禁用安装。
- **安装**：复用 `save_mcp_server`，落库即 `disabled`，用户到「已安装」手动启用（握手与状态展示不变）。
- 关键字为空时只返回精选（不打网络）；有关键字时匹配的精选在前、registry 结果在后，按 command 去重。

## 7. 权限模型

`permissions` 声明在安装/启用时弹窗明示，运行期强制：

| 权限 | 含义 |
| --- | --- |
| `skills` / `mcp` / `rules` | 包含对应声明式内容物（v1） |
| `fs:read` / `fs:write` | 读写当前项目文件（写仍过权限引擎） |
| `agent:run` | 可发起 Agent 任务 |
| `network` | sidecar 可出网（v3） |
| `ui` | 注册 UI 面板（v2） |

## 8. 实施路线

| 阶段 | 内容 | 状态 |
| --- | --- | --- |
| 技能 v1 | 目录扫描 + `/` 触发 + 面板管理 | ✅ 已落地 |
| 插件 v1 | 声明式能力包安装/启停/卸载 | ✅ 已落地 |
| 插件 v1.1 | 插件市场（GitHub 搜索 + 一键安装） | ✅ 已落地 |
| 技能 v1.2 | 技能市场（GitHub `cyan-skill` topic + 一键安装到全局） | ✅ 已落地 |
| 市场 v1.3 | MCP 官方 registry + 精选知名工具一键安装；技能兼容 Claude `*/SKILL.md` 布局、搜索并入 `claude-skill` topic | ✅ 已落地 |
| 插件 v2 | iframe 沙箱 UI + bridge 协议 | 规划中 |
| 插件 v3 | sidecar 后端 + Seatbelt | 规划中 |
