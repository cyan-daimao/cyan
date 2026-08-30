//! WebSearch：DuckDuckGo HTML 端点免 key 搜索（原生兜底工具）。
//!
//! 设计定位（与 MCP 生态配合）：
//! - 用户接了 open-webSearch / wigolo MCP 时，LLM 可用更强引擎（多引擎聚合/中文站）
//! - 未接任何搜索 MCP 时，本工具是零配置兜底（免 key、无外部依赖）
//! - 与 WebFetch 互为搭档：search 拿 URL 列表 → fetch 拿正文

use std::time::Duration;

use crate::domain::DomainError;

/// 搜索超时（15s，比 fetch 短：搜索页轻）
const SEARCH_TIMEOUT: Duration = Duration::from_secs(15);

/// 返回结果条数上限
const MAX_RESULTS: usize = 8;

/// 单条结果摘要截断
const SNIPPET_LIMIT: usize = 300;

/// 单条搜索结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// DuckDuckGo HTML 搜索（免 key）：POST https://html.duckduckgo.com/html/
/// 返回结构化结果（title/url/snippet），无结果时 Ok(vec![])
pub async fn search(query: &str) -> Result<Vec<SearchHit>, DomainError> {
    let query = query.trim();
    if query.is_empty() {
        return Err(DomainError::Validation("搜索关键词不能为空".into()));
    }
    let client = reqwest::Client::builder()
        .timeout(SEARCH_TIMEOUT)
        // DDG 会拦无 UA 的请求
        .user_agent(
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 \
             (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36",
        )
        .build()
        .map_err(|e| DomainError::Validation(format!("HTTP client 构建失败：{e}")))?;

    let resp = client
        .post("https://html.duckduckgo.com/html/")
        .form(&[("q", query), ("kl", "wt-wt")])
        .send()
        .await
        .map_err(|e| DomainError::Validation(format!("搜索请求失败：{e}")))?;
    if !resp.status().is_success() {
        return Err(DomainError::Validation(format!(
            "搜索失败：HTTP {}（DDG 可能限流，稍后重试或接 MCP 搜索工具）",
            resp.status()
        )));
    }
    let html = resp
        .text()
        .await
        .map_err(|e| DomainError::Validation(format!("读取搜索结果失败：{e}")))?;
    Ok(parse_ddg_html(&html))
}

/// 解析 DDG HTML 结果页：
/// 结果块 `<a rel="nofollow" class="result__a" href="…">标题</a>` +
/// 摘要 `<a class="result__snippet" …>摘要</a>`。
/// href 可能是 DDG 跳转链接（//duckduckgo.com/l/?uddg=<urlencoded>），解出真实 URL。
/// 实现：按 `result__a` 标记分段，每段取锚内文本为标题、段内首个 result__snippet 锚文本为摘要。
fn parse_ddg_html(html: &str) -> Vec<SearchHit> {
    // 分段：段 i = 从第 i 个 result__a 到第 i+1 个之前（首段丢弃，前面是页头）
    let marker = "class=\"result__a\"";
    // 分段点 = 锚 tag 开头（<a ... class="result__a"）：marker 往前回退到最近的 "<a"
    let mut starts: Vec<usize> = Vec::new();
    let mut cursor_idx = 0;
    while let Some(rel) = html[cursor_idx..].find(marker) {
        let abs = cursor_idx + rel;
        let anchor_start = html[..abs].rfind("<a").unwrap_or(abs);
        starts.push(anchor_start);
        cursor_idx = abs + marker.len();
    }
    if starts.is_empty() {
        return Vec::new();
    }
    // 段 i = [starts[i], starts[i+1])；末段到结尾
    let mut segments: Vec<&str> = Vec::new();
    for i in 0..starts.len() {
        let end = starts.get(i + 1).copied().unwrap_or(html.len());
        segments.push(&html[starts[i]..end]);
    }

    let mut hits = Vec::new();
    for seg in segments {
        if hits.len() >= MAX_RESULTS {
            break;
        }
        // 段首即标题锚：找 <a ... href="URL">title</a>
        let Some((url, title, _)) = extract_anchor(seg) else {
            continue;
        };
        let url = decode_ddg_redirect(&url);
        if url.is_empty() {
            continue;
        }
        // 摘要：段内 result__snippet 标记往前的 <a 开头
        let snippet = seg
            .find("class=\"result__snippet\"")
            .and_then(|pos| {
                let anchor = seg[..pos].rfind("<a").unwrap_or(pos);
                extract_anchor(&seg[anchor..]).map(|(_, s, _)| s)
            })
            .unwrap_or_default();
        hits.push(SearchHit {
            title: decode_entities(&title),
            url,
            snippet: decode_entities(&snippet),
        });
    }
    hits
}

/// 从 `&rest` 开头附近的 `<a ... href="URL">TITLE</a>` 提取 (href, inner_text, 消耗字节数)
fn extract_anchor(rest: &str) -> Option<(String, String, usize)> {
    let tag_start = rest.find("<a ")?;
    let after_tag = &rest[tag_start..];
    let tag_end = after_tag.find('>')?;
    let tag = &after_tag[..=tag_end];
    // 仅取 class 含 result__a 或 result__snippet 的锚（调用方已定位）
    let href_pos = tag.find("href=\"")? + 6;
    let href_end = tag[href_pos..].find('"')? + href_pos;
    let href = &tag[href_pos..href_end];
    let inner_start = tag_end + 1;
    let close = after_tag[inner_start..].find("</a>")?;
    let title = &after_tag[inner_start..inner_start + close];
    Some((
        href.to_string(),
        title.to_string(),
        tag_start + inner_start + close + 4,
    ))
}

/// DDG 跳转链接 `//duckduckgo.com/l/?uddg=<encoded>&rut=…` → 真实 URL；普通链接原样返回
fn decode_ddg_redirect(href: &str) -> String {
    let h = href.trim();
    if let Some(idx) = h.find("uddg=") {
        let enc = &h[idx + 5..];
        let end = enc.find('&').unwrap_or(enc.len());
        let decoded = urldecode(&enc[..end]);
        return if decoded.starts_with("http") { decoded } else { String::new() };
    }
    // 相对协议或绝对 URL
    let full = if let Some(stripped) = h.strip_prefix("//") {
        format!("https://{stripped}")
    } else {
        h.to_string()
    };
    if full.starts_with("http") { full } else { String::new() }
}

/// 最小 URL 解码（%XX 与 +）
fn urldecode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                if let Ok(v) = u8::from_str_radix(hex, 16) {
                    out.push(v);
                    i += 3;
                } else {
                    out.push(b'%');
                    i += 1;
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// 常见 HTML 实体解码 + 去残留标签
fn decode_entities(s: &str) -> String {
    let no_tags = s
        .replacen("<b>", "", usize::MAX)
        .replacen("</b>", "", usize::MAX)
        .replacen("<strong>", "", usize::MAX)
        .replacen("</strong>", "", usize::MAX);
    no_tags
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
        .trim()
        .chars()
        .take(SNIPPET_LIMIT)
        .collect()
}

/// 格式化输出给 LLM：编号列表，方便 LLM 引用 URL 再用 WebFetch 深读
pub fn format_results(hits: &[SearchHit]) -> String {
    if hits.is_empty() {
        return "无搜索结果".to_string();
    }
    hits.iter()
        .enumerate()
        .map(|(i, h)| {
            if h.snippet.is_empty() {
                format!("{}. {} — {}", i + 1, h.title, h.url)
            } else {
                format!("{}. {} — {}\n   {}", i + 1, h.title, h.url, h.snippet)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<div class="result results_links results_links_deep web-result">
        <h2 class="result__title">
            <a rel="nofollow" class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fa&amp;rut=abc">Example <b>Title</b> A</a>
        </h2>
        <a class="result__snippet" href="//example.com/a">First &amp; snippet <b>text</b></a>
        </div>
        <div class="result">
        <h2 class="result__title">
            <a rel="nofollow" class="result__a" href="https://direct.example.com/b">Direct B</a>
        </h2>
        <a class="result__snippet" href="//example.com/b">Second snippet</a>
        </div>"#;

    
    
    
    
    #[test]
    fn parse_ddg_html_extracts_results() {
        let hits = parse_ddg_html(SAMPLE);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].title, "Example Title A");
        assert_eq!(hits[0].url, "https://example.com/a");
        assert_eq!(hits[0].snippet, "First & snippet text");
        assert_eq!(hits[1].url, "https://direct.example.com/b");
        assert_eq!(hits[1].snippet, "Second snippet");
    }

    #[test]
    fn parse_ddg_html_empty_page() {
        assert!(parse_ddg_html("<html><body>no results</body></html>").is_empty());
    }

    #[test]
    fn decode_ddg_redirect_variants() {
        assert_eq!(
            decode_ddg_redirect("//duckduckgo.com/l/?uddg=https%3A%2F%2Fx.dev%2Fa&rut=1"),
            "https://x.dev/a"
        );
        assert_eq!(decode_ddg_redirect("https://plain.dev"), "https://plain.dev");
        assert_eq!(decode_ddg_redirect("javascript:void(0)"), "");
    }

    #[test]
    fn format_results_numbered() {
        let hits = vec![SearchHit {
            title: "T".into(),
            url: "https://x".into(),
            snippet: "s".into(),
        }];
        let out = format_results(&hits);
        assert!(out.starts_with("1. T — https://x"));
        assert_eq!(format_results(&[]), "无搜索结果");
    }
}
