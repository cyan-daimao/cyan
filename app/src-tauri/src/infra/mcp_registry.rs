//! MCP 官方 registry 搜索 + 精选列表（协议结构不出层，错误映射参照 infra/plugin/github.rs）。
//! registry API：`GET https://registry.modelcontextprotocol.io/v0/servers?search=<kw>&limit=30`

use std::time::Duration;

/// 搜索超时（30s）
const SEARCH_TIMEOUT: Duration = Duration::from_secs(30);

/// registry 官方元数据键（isLatest 标记）
const OFFICIAL_META_KEY: &str = "io.modelcontextprotocol.registry/official";

/// MCP 市场条目（infra 传输结构，application 转 BO）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpMarketItem {
    /// 服务器标识（registry name 或精选短名）
    pub name: String,
    /// 展示标题
    pub title: String,
    /// 描述
    pub description: String,
    /// 版本
    pub version: String,
    /// 安装命令（无可用 stdio 包为 None，前端禁用安装）
    pub command: Option<String>,
    /// 来源（featured / registry）
    pub source: &'static str,
    /// 主页（GitHub 仓库地址）
    pub homepage: Option<String>,
}

/// 精选知名 MCP 工具（包名已逐个验证真实存在）
pub fn featured_servers() -> Vec<McpMarketItem> {
    let featured = |name: &str, title: &str, desc: &str, command: &str, repo: &str| McpMarketItem {
        name: name.into(),
        title: title.into(),
        description: desc.into(),
        version: "latest".into(),
        command: Some(command.into()),
        source: "featured",
        homepage: Some(format!("https://github.com/{repo}")),
    };
    vec![
        featured("context7", "Context7", "为 LLM 提供最新官方文档", "npx -y @upstash/context7-mcp", "upstash/context7"),
        featured("playwright", "Playwright", "浏览器自动化与截图", "npx -y @playwright/mcp@latest", "microsoft/playwright-mcp"),
        featured("chrome-devtools", "Chrome DevTools", "调试、性能与网络分析", "npx -y chrome-devtools-mcp@latest", "ChromeDevTools/chrome-devtools-mcp"),
        featured("filesystem", "Filesystem", "受控文件系统访问", "npx -y @modelcontextprotocol/server-filesystem", "modelcontextprotocol/servers"),
        featured("github", "GitHub", "仓库/Issue/PR 操作", "npx -y @modelcontextprotocol/server-github", "modelcontextprotocol/servers"),
        featured("memory", "Memory", "知识图谱式长期记忆", "npx -y @modelcontextprotocol/server-memory", "modelcontextprotocol/servers"),
        featured("fetch", "Fetch", "网页抓取转 Markdown", "uvx mcp-server-fetch", "modelcontextprotocol/servers"),
        featured("time", "Time", "时间与时区换算", "uvx mcp-server-time", "modelcontextprotocol/servers"),
    ]
}

/// 搜索官方 registry（isLatest 过滤 + 按 name 去重）
pub async fn search_registry(keyword: &str) -> anyhow::Result<Vec<McpMarketItem>> {
    let client = reqwest::Client::builder()
        .user_agent("cyan-app")
        .timeout(SEARCH_TIMEOUT)
        .build()?;
    let resp = client
        .get("https://registry.modelcontextprotocol.io/v0/servers")
        .query(&[("search", keyword.trim()), ("limit", "30")])
        .send()
        .await?;
    if !resp.status().is_success() {
        return Err(match resp.status().as_u16() {
            403 | 429 => anyhow::anyhow!("MCP registry 限流，请稍后重试"),
            s => anyhow::anyhow!("MCP registry 请求失败：HTTP {s}"),
        });
    }
    let text = resp.text().await?;
    parse_registry_response(&text)
}

// ---- registry 协议结构（本层私有） ----

/// /v0/servers 响应体
#[derive(Debug, serde::Deserialize)]
struct RegistryResponse {
    /// 命中条目
    #[serde(default)]
    servers: Vec<RegistryEntry>,
}

/// 条目（server + 元数据）
#[derive(Debug, serde::Deserialize)]
struct RegistryEntry {
    /// 服务器声明
    server: RegistryServer,
    /// 元数据（含官方 isLatest 标记）
    #[serde(default)]
    _meta: serde_json::Value,
}

/// 服务器声明
#[derive(Debug, serde::Deserialize)]
struct RegistryServer {
    /// 名称（发布标识，如 `io.github.x/y`）
    name: String,
    /// 标题
    #[serde(default)]
    title: Option<String>,
    /// 描述
    #[serde(default)]
    description: Option<String>,
    /// 版本
    #[serde(default)]
    version: Option<String>,
    /// 安装包列表
    #[serde(default)]
    packages: Vec<RegistryPackage>,
}

/// 安装包声明
#[derive(Debug, serde::Deserialize)]
struct RegistryPackage {
    /// 包仓库类型（npm/pypi/oci）
    #[serde(rename = "registryType")]
    registry_type: String,
    /// 包名
    identifier: String,
    /// 传输方式
    #[serde(default)]
    transport: Option<RegistryTransport>,
}

/// 传输方式
#[derive(Debug, serde::Deserialize)]
struct RegistryTransport {
    /// 类型（stdio/sse/...）
    #[serde(rename = "type")]
    kind: String,
}

/// packages → 安装命令：npm → `npx -y <id>`，pypi → `uvx <id>`；无可用 stdio 包 → None
fn command_of(packages: &[RegistryPackage]) -> Option<String> {
    packages
        .iter()
        .filter(|p| p.transport.as_ref().map(|t| t.kind == "stdio").unwrap_or(true))
        .find_map(|p| match p.registry_type.as_str() {
            "npm" => Some(format!("npx -y {}", p.identifier)),
            "pypi" => Some(format!("uvx {}", p.identifier)),
            _ => None,
        })
}

/// 解析 registry 响应（纯函数）：isLatest 过滤 + 按 name 去重 + command 映射
fn parse_registry_response(text: &str) -> anyhow::Result<Vec<McpMarketItem>> {
    let resp: RegistryResponse = serde_json::from_str(text)?;
    let mut seen = std::collections::HashSet::new();
    let mut items = Vec::new();
    for entry in resp.servers {
        // 只保留官方标记的最新版本
        let is_latest = entry
            ._meta
            .get(OFFICIAL_META_KEY)
            .and_then(|m| m.get("isLatest"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !is_latest || !seen.insert(entry.server.name.clone()) {
            continue;
        }
        items.push(McpMarketItem {
            title: entry.server.title.clone().unwrap_or_else(|| entry.server.name.clone()),
            name: entry.server.name,
            description: entry.server.description.unwrap_or_default(),
            version: entry.server.version.unwrap_or_default(),
            command: command_of(&entry.server.packages),
            source: "registry",
            homepage: None,
        });
    }
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn featured_servers_eight_verified_items() {
        let items = featured_servers();
        assert_eq!(items.len(), 8);
        assert!(items.iter().all(|i| i.command.is_some() && i.source == "featured"));
        assert!(items.iter().any(|i| i.command.as_deref() == Some("npx -y @upstash/context7-mcp")));
        assert!(items.iter().any(|i| i.command.as_deref() == Some("uvx mcp-server-fetch")));
    }

    #[test]
    fn parse_registry_filters_latest_and_dedupes_by_name() {
        let sample = r#"{
          "servers": [
            {
              "server": {
                "name": "io.github.upstash/context7",
                "title": "Context7",
                "description": "最新文档",
                "version": "1.0.0",
                "packages": [{"registryType":"npm","identifier":"@upstash/context7-mcp","transport":{"type":"stdio"}}]
              },
              "_meta": {"io.modelcontextprotocol.registry/official": {"isLatest": false}}
            },
            {
              "server": {
                "name": "io.github.upstash/context7",
                "title": "Context7",
                "description": "最新文档",
                "version": "2.0.0",
                "packages": [{"registryType":"npm","identifier":"@upstash/context7-mcp","transport":{"type":"stdio"}}]
              },
              "_meta": {"io.modelcontextprotocol.registry/official": {"isLatest": true}}
            },
            {
              "server": {
                "name": "io.github.x/pypi-tool",
                "description": "pypi 工具",
                "version": "0.1.0",
                "packages": [{"registryType":"pypi","identifier":"pypi-tool","transport":{"type":"stdio"}}]
              },
              "_meta": {"io.modelcontextprotocol.registry/official": {"isLatest": true}}
            },
            {
              "server": {
                "name": "io.github.y/remote-only",
                "description": "无 stdio 包",
                "version": "0.1.0",
                "packages": [{"registryType":"oci","identifier":"ghcr.io/y/x","transport":{"type":"stdio"}}],
                "remotes": [{"type":"sse","url":"https://x.dev/sse"}]
              },
              "_meta": {"io.modelcontextprotocol.registry/official": {"isLatest": true}}
            }
          ],
          "metadata": {"count": 4}
        }"#;
        let items = parse_registry_response(sample).unwrap();
        assert_eq!(items.len(), 3, "旧版本过滤 + 按 name 去重");
        let ctx7 = &items[0];
        assert_eq!(ctx7.version, "2.0.0", "保留 isLatest 版本");
        assert_eq!(ctx7.command.as_deref(), Some("npx -y @upstash/context7-mcp"));
        assert_eq!(items[1].command.as_deref(), Some("uvx pypi-tool"));
        assert_eq!(items[2].command, None, "oci 无可用 stdio 包 → None");
        assert!(items.iter().all(|i| i.source == "registry"));
    }

    #[test]
    fn parse_registry_invalid_json_err() {
        assert!(parse_registry_response("not json").is_err());
        assert!(parse_registry_response(r#"{"servers":[]}"#).unwrap().is_empty());
    }
}
