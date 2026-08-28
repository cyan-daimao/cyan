//! GitHub 插件市场：topic:cyan-plugin 仓库搜索 + codeload zip 下载。
//! GitHub API 协议结构仅此层使用，不出层；网络错误映射为友好文案（application 转 3xxx）。

use std::io::Write as _;
use std::time::Duration;

use futures::StreamExt;

use crate::domain::DomainError;

/// 搜索超时（30s）
const SEARCH_TIMEOUT: Duration = Duration::from_secs(30);

/// 下载超时（60s）
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(60);

/// GitHub 强制要求的 User-Agent
const USER_AGENT: &str = "cyan-app";

/// 市场条目（infra 传输结构，application 转 BO）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketItem {
    /// 仓库全名（owner/repo）
    pub full_name: String,
    /// 描述（可空）
    pub description: Option<String>,
    /// star 数
    pub stars: i64,
    /// 作者（owner.login）
    pub author: String,
    /// 仓库页面 URL
    pub url: String,
}

/// 校验仓库全名格式 `owner/repo`（owner/repo 仅允许字母数字与 `-_.`，防路径注入）
pub fn validate_full_name(full_name: &str) -> Result<(&str, &str), DomainError> {
    let (owner, repo) = full_name
        .split_once('/')
        .ok_or_else(|| DomainError::Validation("仓库全名须为 owner/repo 格式".into()))?;
    let legal = |s: &str| {
        !s.is_empty()
            && s.len() <= 100
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
            && !s.starts_with(['-', '.'])
            && !s.contains("..")
    };
    if !legal(owner) || !legal(repo) || repo.contains('/') {
        return Err(DomainError::Validation(format!(
            "非法仓库全名：{full_name}"
        )));
    }
    Ok((owner, repo))
}

/// GitHub 错误状态 → 友好文案（403/429 为限流）
fn status_error(status: reqwest::StatusCode) -> anyhow::Error {
    match status.as_u16() {
        403 | 429 => anyhow::anyhow!("GitHub API 限流（未认证 10 次/分钟），请稍后重试"),
        _ => anyhow::anyhow!("GitHub 请求失败：HTTP {status}"),
    }
}

/// 构建带 User-Agent 的 client
fn client(timeout: Duration) -> anyhow::Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(timeout)
        .build()?)
}

/// 按 topic 搜索 GitHub 仓库（按 stars 排序，取前 20）
pub async fn search_market(keyword: &str, topic: &str) -> anyhow::Result<Vec<MarketItem>> {
    let q = if keyword.trim().is_empty() {
        format!("topic:{topic}")
    } else {
        format!("{} topic:{topic}", keyword.trim())
    };
    let resp = client(SEARCH_TIMEOUT)?
        .get("https://api.github.com/search/repositories")
        .query(&[("q", q.as_str()), ("sort", "stars"), ("per_page", "20")])
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()
        .await?;
    if !resp.status().is_success() {
        return Err(status_error(resp.status()));
    }
    let text = resp.text().await?;
    parse_search_response(&text)
}

/// 搜索插件市场（topic:cyan-plugin）
pub async fn search_plugins(keyword: &str) -> anyhow::Result<Vec<MarketItem>> {
    search_market(keyword, "cyan-plugin").await
}

/// 搜索技能市场（topic:cyan-skill）
pub async fn search_skills(keyword: &str) -> anyhow::Result<Vec<MarketItem>> {
    search_market(keyword, "cyan-skill").await
}

/// 下载仓库 zip（codeload HEAD）到临时文件；调用方持有 TempPath 直至安装完成
pub async fn download_repo_zip(full_name: &str) -> anyhow::Result<tempfile::TempPath> {
    let (owner, repo) = validate_full_name(full_name)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let url = format!("https://codeload.github.com/{owner}/{repo}/zip/HEAD");
    let resp = client(DOWNLOAD_TIMEOUT)?.get(&url).send().await?;
    if !resp.status().is_success() {
        return Err(status_error(resp.status()));
    }
    let mut tmp = tempfile::NamedTempFile::new()?;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        tmp.write_all(&chunk)?;
    }
    tmp.flush()?;
    Ok(tmp.into_temp_path())
}

// ---- GitHub API 协议结构（本层私有） ----

/// search/repositories 响应体
#[derive(Debug, serde::Deserialize)]
struct SearchResponse {
    /// 命中条目
    #[serde(default)]
    items: Vec<SearchItem>,
}

/// 条目
#[derive(Debug, serde::Deserialize)]
struct SearchItem {
    /// 仓库全名
    full_name: String,
    /// 描述
    description: Option<String>,
    /// star 数
    #[serde(default)]
    stargazers_count: i64,
    /// 作者
    owner: SearchOwner,
    /// 页面 URL
    html_url: String,
}

/// 作者
#[derive(Debug, serde::Deserialize)]
struct SearchOwner {
    /// 登录名
    login: String,
}

/// 解析搜索响应（纯函数，便于测试）
fn parse_search_response(text: &str) -> anyhow::Result<Vec<MarketItem>> {
    let resp: SearchResponse = serde_json::from_str(text)?;
    Ok(resp
        .items
        .into_iter()
        .map(|i| MarketItem {
            full_name: i.full_name,
            description: i.description,
            stars: i.stargazers_count,
            author: i.owner.login,
            url: i.html_url,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_full_name_rules() {
        assert!(validate_full_name("owner/repo").is_ok());
        assert!(validate_full_name("cy-an/cyan_plugin.v1").is_ok());
        assert!(validate_full_name("").is_err());
        assert!(validate_full_name("no-slash").is_err());
        assert!(validate_full_name("a/b/c").is_err());
        assert!(validate_full_name("../evil/repo").is_err());
        assert!(validate_full_name("owner/../x").is_err());
        assert!(validate_full_name("-bad/repo").is_err());
        assert!(validate_full_name("owner/").is_err());
        assert!(validate_full_name("/repo").is_err());
    }

    #[test]
    fn parse_search_response_sample() {
        let sample = r#"{
          "total_count": 2,
          "items": [
            {
              "full_name": "cy/cyan-weekly",
              "description": "周报插件",
              "stargazers_count": 42,
              "owner": { "login": "cy" },
              "html_url": "https://github.com/cy/cyan-weekly"
            },
            {
              "full_name": "someone/toolkit",
              "description": null,
              "stargazers_count": 0,
              "owner": { "login": "someone" },
              "html_url": "https://github.com/someone/toolkit"
            }
          ]
        }"#;
        let items = parse_search_response(sample).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].full_name, "cy/cyan-weekly");
        assert_eq!(items[0].description.as_deref(), Some("周报插件"));
        assert_eq!(items[0].stars, 42);
        assert_eq!(items[0].author, "cy");
        assert_eq!(items[1].description, None);
    }

    #[test]
    fn parse_search_response_empty_and_invalid() {
        assert!(parse_search_response(r#"{"items":[]}"#).unwrap().is_empty());
        assert!(parse_search_response("not json").is_err());
    }

    #[test]
    fn status_error_maps_rate_limit() {
        let e = status_error(reqwest::StatusCode::FORBIDDEN);
        assert!(e.to_string().contains("限流"));
        let e = status_error(reqwest::StatusCode::TOO_MANY_REQUESTS);
        assert!(e.to_string().contains("限流"));
        let e = status_error(reqwest::StatusCode::INTERNAL_SERVER_ERROR);
        assert!(e.to_string().contains("500"));
    }
}
