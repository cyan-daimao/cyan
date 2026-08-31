//! Gitee 插件市场：`/api/v5/repos/{owner}/{repo}` 详情 + `repository/archive/{branch}.zip` 下载。
//! 与 github.rs 同构（MarketItem 共用）；Gitee 搜索接口匿名调用常被风控拦成空数组，
//! 故不做网络搜索——前端把关键字当作 owner/repo 直达安装（同 GitHub 市场的「直接安装」交互）。

use std::io::Write as _;
use std::time::Duration;

use futures::StreamExt;

use crate::infra::plugin::github::{validate_full_name, MarketItem};

/// 详情接口超时（15s，轻量）
const DETAIL_TIMEOUT: Duration = Duration::from_secs(15);

/// 下载超时（120s，Gitee 服务器在国内，放宽到 GitHub 的 2 倍）
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(120);

/// UA（Gitee 不强制，带上有助于风控放行）
const USER_AGENT: &str = "cyan-app";

/// 构建带 User-Agent 的 client
fn client(timeout: Duration) -> anyhow::Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(timeout)
        .build()?)
}

/// 详情解析结果：市场条目 + 默认分支名（拼归档 zip 地址用）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoMeta {
    /// 市场条目
    pub item: MarketItem,
    /// 默认分支（master / main …）
    pub default_branch: String,
}

/// 拉取仓库详情（市场卡片数据 + 默认分支）
pub async fn repo_detail(full_name: &str) -> anyhow::Result<RepoMeta> {
    let (owner, repo) =
        validate_full_name(full_name).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let url = format!("https://gitee.com/api/v5/repos/{owner}/{repo}");
    let resp = client(DETAIL_TIMEOUT)?.get(&url).send().await?;
    let status = resp.status();
    if !status.is_success() {
        return Err(match status.as_u16() {
            404 => anyhow::anyhow!("Gitee 仓库不存在：{full_name}"),
            403 => anyhow::anyhow!("Gitee 请求被限流，请稍后重试"),
            _ => anyhow::anyhow!("Gitee 请求失败：HTTP {status}"),
        });
    }
    let text = resp.text().await?;
    parse_repo_detail(&text, full_name)
}

/// 下载仓库 zip（默认分支归档）到临时文件；调用方持有 TempPath 直至安装完成
pub async fn download_repo_zip(full_name: &str) -> anyhow::Result<tempfile::TempPath> {
    let (owner, repo) =
        validate_full_name(full_name).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    // 先取默认分支（master/main 不固定），同时完成存在性校验
    let meta = repo_detail(full_name).await?;
    let url = format!(
        "https://gitee.com/{owner}/{repo}/repository/archive/{}.zip",
        meta.default_branch
    );
    let resp = client(DOWNLOAD_TIMEOUT)?.get(&url).send().await?;
    let status = resp.status();
    if !status.is_success() {
        return Err(anyhow::anyhow!("Gitee 仓库下载失败：HTTP {status}"));
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

// ---- Gitee API 协议结构（本层私有） ----

/// repos/{owner}/{repo} 响应体（仅取所需字段）
#[derive(Debug, serde::Deserialize)]
struct RepoDetail {
    /// 描述（可空）
    #[serde(default)]
    description: Option<String>,
    /// star 数（Gitee 详情字段稳定返回；缺省按 0）
    #[serde(default)]
    stargazers_count: i64,
    /// 作者
    #[serde(default)]
    owner: Option<RepoOwner>,
    /// 页面 URL
    #[serde(default)]
    html_url: Option<String>,
    /// 默认分支（master / main …）
    #[serde(default)]
    default_branch: Option<String>,
}

/// 作者（Gitee 详情里 owner.path 即 owner 段，与 GitHub login 等价）
#[derive(Debug, serde::Deserialize)]
struct RepoOwner {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    login: Option<String>,
}

/// 解析仓库详情（纯函数，便于测试）：owner/repo 以入参为准（已过 validate_full_name），
/// 接口字段仅补充展示信息；default_branch 缺省 master。
fn parse_repo_detail(text: &str, full_name: &str) -> anyhow::Result<RepoMeta> {
    let d: RepoDetail = serde_json::from_str(text)?;
    let item = MarketItem {
        full_name: full_name.to_string(),
        description: d.description.filter(|s| !s.trim().is_empty()),
        stars: d.stargazers_count,
        author: d
            .owner
            .and_then(|o| o.path.or(o.login))
            .unwrap_or_default(),
        url: d
            .html_url
            .unwrap_or_else(|| format!("https://gitee.com/{full_name}")),
    };
    Ok(RepoMeta {
        item,
        default_branch: d
            .default_branch
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "master".to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_repo_detail_sample() {
        let sample = r#"{
            "id": 5198406,
            "full_name": "DCloud/uni-app",
            "path": "uni-app",
            "description": "uni-app 框架镜像",
            "stargazers_count": 1598,
            "owner": {"login": "hbcui1984", "path": "dcloud"},
            "html_url": "https://gitee.com/dcloud/uni-app",
            "default_branch": "master"
        }"#;
        let meta = parse_repo_detail(sample, "dcloud/uni-app").unwrap();
        assert_eq!(meta.item.full_name, "dcloud/uni-app");
        assert_eq!(meta.item.description.as_deref(), Some("uni-app 框架镜像"));
        assert_eq!(meta.item.stars, 1598);
        assert_eq!(meta.item.author, "dcloud");
        assert_eq!(meta.item.url, "https://gitee.com/dcloud/uni-app");
        assert_eq!(meta.default_branch, "master");
    }

    #[test]
    fn parse_repo_detail_minimal() {
        // 最小响应：缺 owner/描述/html_url/default_branch 时兜底不报错
        let meta = parse_repo_detail(r#"{"path":"repo"}"#, "cy/repo").unwrap();
        assert_eq!(meta.item.full_name, "cy/repo");
        assert_eq!(meta.item.description, None);
        assert_eq!(meta.item.stars, 0);
        assert_eq!(meta.item.author, "");
        assert_eq!(meta.item.url, "https://gitee.com/cy/repo");
        assert_eq!(meta.default_branch, "master", "缺省分支兜底 master");
    }

    #[test]
    fn parse_repo_detail_invalid_json() {
        assert!(parse_repo_detail("not json", "a/b").is_err());
    }

    #[test]
    fn repo_detail_rejects_bad_full_name() {
        // 非法 owner/repo 不发网络请求直接报错
        let rt = tokio::runtime::Runtime::new().unwrap();
        let err = rt.block_on(repo_detail("../evil"));
        assert!(err.is_err());
    }
}
