//! API Key 安全存储：`~/.cyan/keys.json`（权限 0600，仅当前用户可读写）。
//! 说明：原设计为 OS keychain，但 macOS 数据保护 keychain 会把条目绑定到创建它的二进制，
//! adhoc 签名每次构建都变，重建后旧条目读不出来（表现为「未配置 API Key」）。
//! 开发/迭代期改用用户目录文件存储；正式发布签名后可再切回 keychain（TECH_DESIGN 6.6）。

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::domain::config::ModelConfig;

/// 条目键：`model/<name>`
fn entry_key(model_name: &str) -> String {
    format!("model/{model_name}")
}

/// 密钥文件路径；测试可用 CYAN_KEYS_FILE 覆盖
fn keys_file() -> anyhow::Result<PathBuf> {
    if let Ok(p) = std::env::var("CYAN_KEYS_FILE") {
        return Ok(PathBuf::from(p));
    }
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("无法定位用户主目录"))?;
    Ok(home.join(".cyan").join("keys.json"))
}

/// 读取全部条目
fn load_all() -> anyhow::Result<BTreeMap<String, String>> {
    let path = keys_file()?;
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let raw = std::fs::read_to_string(&path)?;
    let map = serde_json::from_str(&raw).unwrap_or_default();
    Ok(map)
}

/// 写回全部条目，文件权限固定 0600
fn save_all(map: &BTreeMap<String, String>) -> anyhow::Result<()> {
    let path = keys_file()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let raw = serde_json::to_string_pretty(map)?;
    std::fs::write(&path, raw)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

/// 写入 API Key，返回库内引用串
pub fn store_api_key(model_name: &str, api_key: &str) -> anyhow::Result<String> {
    let mut map = load_all()?;
    map.insert(entry_key(model_name), api_key.to_string());
    save_all(&map)?;
    Ok(ModelConfig::keychain_ref(model_name))
}

/// 读取 API Key；不存在时报错（上层映射为「未配置 API Key」）
pub fn load_api_key(model_name: &str) -> anyhow::Result<String> {
    let map = load_all()?;
    map.get(&entry_key(model_name))
        .cloned()
        .filter(|k| !k.is_empty())
        .ok_or_else(|| anyhow::anyhow!("模型 {model_name} 未配置 API Key"))
}

/// 删除 API Key（忽略不存在）
pub fn delete_api_key(model_name: &str) {
    if let Ok(mut map) = load_all() {
        if map.remove(&entry_key(model_name)).is_some() {
            let _ = save_all(&map);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 用临时文件覆盖存储路径，验证存-读-删闭环
    #[test]
    fn store_load_delete_roundtrip() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_path_buf();
        std::fs::remove_file(&path).unwrap();
        std::env::set_var("CYAN_KEYS_FILE", &path);

        store_api_key("kimi-k2.5", "sk-test-123").unwrap();
        assert_eq!(load_api_key("kimi-k2.5").unwrap(), "sk-test-123");

        // 文件权限必须为 0600
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }

        delete_api_key("kimi-k2.5");
        assert!(load_api_key("kimi-k2.5").is_err());

        std::env::remove_var("CYAN_KEYS_FILE");
    }
}
