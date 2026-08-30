//! WebFetch：reqwest（rustls）网络访问，协议适配不出层。

use std::time::Duration;

use crate::domain::DomainError;

/// 抓取超时（30s）
const FETCH_TIMEOUT: Duration = Duration::from_secs(30);

/// 返回文本截断上限（约 20KB）
const FETCH_TEXT_LIMIT: usize = 20 * 1024;

/// 抓取 URL 文本内容：HTML 剥离标签，截断 ~20KB
pub async fn fetch_url(url: &str) -> Result<String, DomainError> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(DomainError::Validation(
            "URL 必须以 http:// 或 https:// 开头".into(),
        ));
    }
    let client = reqwest::Client::builder()
        .timeout(FETCH_TIMEOUT)
        .build()
        .map_err(|e| DomainError::Validation(format!("HTTP client 构建失败：{e}")))?;
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| DomainError::Validation(format!("请求失败：{e}")))?;
    if !resp.status().is_success() {
        return Err(DomainError::Validation(format!(
            "HTTP {}：请求失败",
            resp.status()
        )));
    }
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| DomainError::Validation(format!("读取响应失败：{e}")))?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let text = if content_type.contains("html") || text.trim_start().starts_with('<') {
        strip_html(&text)
    } else {
        text
    };
    Ok(truncate_text(&text, FETCH_TEXT_LIMIT))
}

/// 简单 HTML 去标签：去 script/style 块 → 去标签 → 折叠空白（不引 html 解析库）
pub(crate) fn strip_html(input: &str) -> String {
    let script_re = regex::Regex::new(r"(?is)<(script|style)[^>]*>.*?</(script|style)>")
        .expect("script/style 正则为常量");
    let tag_re = regex::Regex::new(r"(?s)<[^>]+>").expect("标签正则为常量");
    let blank_re = regex::Regex::new(r"\n{3,}").expect("空白正则为常量");
    let ws_re = regex::Regex::new(r"[ \t]+").expect("空格正则为常量");
    let no_scripts = script_re.replace_all(input, "");
    let no_tags = tag_re.replace_all(&no_scripts, "");
    let joined = no_tags
        .lines()
        .map(|l| ws_re.replace_all(l, " ").trim().to_string())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    blank_re.replace_all(&joined, "\n\n").to_string()
}

/// 按字符截断到上限
fn truncate_text(s: &str, limit: usize) -> String {
    if s.len() <= limit {
        return s.to_string();
    }
    let mut end = limit;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…\n[内容已截断]", &s[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_html_removes_tags_and_scripts() {
        let html = "<html><head><style>body{color:red}</style></head>\
                    <body><h1>标题</h1><p>正文 <b>加粗</b></p>\
                    <script>alert(1)</script></body></html>";
        let text = strip_html(html);
        assert!(text.contains("标题"));
        assert!(text.contains("正文 加粗"));
        assert!(!text.contains("alert"));
        assert!(!text.contains("color:red"));
        assert!(!text.contains('<'));
    }

    #[test]
    fn truncate_text_respects_char_boundary() {
        let s = "汉".repeat(20 * 1024);
        let out = truncate_text(&s, FETCH_TEXT_LIMIT);
        assert!(out.contains("[内容已截断]"));
        let short = "abc";
        assert_eq!(truncate_text(short, FETCH_TEXT_LIMIT), "abc");
    }

    #[tokio::test]
    async fn fetch_url_rejects_bad_scheme() {
        let err = fetch_url("ftp://example.com").await.unwrap_err();
        assert!(matches!(err, DomainError::Validation(_)));
    }
}
