//! 通用工具函数：base64 解码 + 截图落盘目录。
//! （原为 CDP 页面操作层；CDP 方案移除后仅保留被 MCP image 落盘复用的部分）

use std::path::PathBuf;

use crate::infra::db::datasource::cyan_home;

/// 截图保存目录：`~/.cyan/screenshots/`（MCP image 落盘共用）
pub fn screenshot_dir() -> PathBuf {
    cyan_home()
        .map(|h| h.join("screenshots"))
        .unwrap_or_else(|_| PathBuf::from(".cyan/screenshots"))
}

/// 简易 base64 解码（无依赖；MCP image data 是标准字母表）
pub fn base64_decode_pub(s: &str) -> Result<Vec<u8>, String> {
    base64_decode(s)
}

/// 简易 base64 解码（无依赖）
fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut lookup = [255u8; 256];
    for (i, &c) in TABLE.iter().enumerate() {
        lookup[c as usize] = i as u8;
    }
    let s = s.trim_end_matches('=');
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut buf: u32 = 0;
    let mut bits = 0u32;
    for &c in s.as_bytes() {
        let v = lookup[c as usize];
        if v == 255 {
            return Err("base64 数据含非法字符".into());
        }
        buf = (buf << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_roundtrip() {
        // "Hello" → SGVsbG8=
        assert_eq!(base64_decode("SGVsbG8=").unwrap(), b"Hello");
        // 空
        assert!(base64_decode("").unwrap().is_empty());
        // padding
        assert_eq!(base64_decode("QQ==").unwrap(), b"A");
        // 非法字符
        assert!(base64_decode("a*bc").is_err());
    }
}
