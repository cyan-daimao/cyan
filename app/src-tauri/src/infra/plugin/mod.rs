//! 插件包安装：zip 解压 / 目录复制到 `<plugins_dir>/<name>/`，manifest 与内容物（mcp.json/rules.json/skills）解析。
//! 包内 JSON 协议结构仅此层使用，不出层。

use std::path::{Path, PathBuf};

use crate::domain::plugin::PluginManifest;
use crate::domain::DomainError;

pub mod github;

/// MCP 声明（mcp.json 条目）
#[derive(Debug, Clone, serde::Deserialize)]
pub struct McpDecl {
    /// 服务器名
    pub name: String,
    /// 启动命令
    pub command: String,
}

/// 权限规则声明（rules.json 条目）
#[derive(Debug, Clone, serde::Deserialize)]
pub struct RuleDecl {
    /// 工具名
    pub tool: String,
    /// glob 匹配模式
    pub pattern: String,
    /// 动作（allow/ask/deny）
    pub action: String,
    /// 匹配顺序（缺省 0）
    #[serde(default)]
    pub sort: i64,
}

/// 从包源读取 manifest：zip 读内嵌条目，目录读文件
pub fn read_manifest_from_source(source: &Path) -> Result<PluginManifest, DomainError> {
    let text = if source.is_dir() {
        let path = source.join("manifest.json");
        std::fs::read_to_string(&path)
            .map_err(|e| DomainError::Validation(format!("读取 manifest.json 失败：{e}")))?
    } else if source.is_file() && source.extension().and_then(|e| e.to_str()) == Some("zip") {
        let file = std::fs::File::open(source)
            .map_err(|e| DomainError::Validation(format!("打开插件包失败：{e}")))?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| DomainError::Validation(format!("非法 zip 包：{e}")))?;
        // 允许顶层单目录包裹（如 my-plugin/manifest.json）
        let entry = find_entry_name(&mut archive, "manifest.json")
            .ok_or_else(|| DomainError::Validation("包内缺少 manifest.json".into()))?;
        let mut entry_file = archive
            .by_name(&entry)
            .map_err(|e| DomainError::Validation(format!("读取包内 manifest.json 失败：{e}")))?;
        let mut buf = String::new();
        std::io::Read::read_to_string(&mut entry_file, &mut buf)
            .map_err(|e| DomainError::Validation(format!("读取包内 manifest.json 失败：{e}")))?;
        buf
    } else {
        return Err(DomainError::Validation(format!(
            "插件源须为 zip 文件或目录：{}",
            source.display()
        )));
    };
    parse_manifest(&text)
}

/// 解析 manifest JSON 并校验
pub fn parse_manifest(text: &str) -> Result<PluginManifest, DomainError> {
    let manifest: PluginManifest = serde_json::from_str(text)
        .map_err(|e| DomainError::Validation(format!("manifest.json 格式非法：{e}")))?;
    manifest.validate()?;
    Ok(manifest)
}

/// 在 zip 中定位条目（允许顶层单目录包裹），返回完整条目名
fn find_entry_name(archive: &mut zip::ZipArchive<std::fs::File>, file_name: &str) -> Option<String> {
    for i in 0..archive.len() {
        let Ok(f) = archive.by_index(i) else { continue };
        let name = f.name().to_string();
        if name == file_name || name.ends_with(&format!("/{file_name}")) {
            return Some(name);
        }
    }
    None
}

/// 解压 zip 到目标目录（剥离顶层包裹目录、防 zip-slip；目标目录自动创建）
pub fn unzip_to(zip_path: &Path, target: &Path) -> Result<(), DomainError> {
    std::fs::create_dir_all(target)
        .map_err(|e| DomainError::Validation(format!("创建目录失败：{e}")))?;
    extract_zip(zip_path, target)
}

/// 安装包到 `<plugins_dir>/<name>/`：zip 解压 / 目录复制；目标已存在报冲突
pub fn extract_package(source: &Path, plugins_dir: &Path, name: &str) -> Result<PathBuf, DomainError> {
    let target = plugins_dir.join(name);
    if target.exists() {
        return Err(DomainError::Conflict(format!("插件目录已存在：{name}")));
    }
    std::fs::create_dir_all(&target)
        .map_err(|e| DomainError::Validation(format!("创建插件目录失败：{e}")))?;
    let result = if source.is_dir() {
        copy_dir(source, &target)
    } else {
        extract_zip(source, &target)
    };
    if let Err(e) = result {
        // 失败回滚：不留半成品目录
        let _ = std::fs::remove_dir_all(&target);
        return Err(e);
    }
    Ok(target)
}

/// 递归复制目录
fn copy_dir(src: &Path, dst: &Path) -> Result<(), DomainError> {
    let entries = std::fs::read_dir(src)
        .map_err(|e| DomainError::Validation(format!("读取插件目录失败：{e}")))?;
    for entry in entries.flatten() {
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            std::fs::create_dir_all(&to)
                .map_err(|e| DomainError::Validation(format!("创建目录失败：{e}")))?;
            copy_dir(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)
                .map_err(|e| DomainError::Validation(format!("复制文件失败：{e}")))?;
        }
    }
    Ok(())
}

/// zip 解压（防 zip-slip：拒绝跳出目标目录的条目；容忍顶层单目录包裹并剥掉）
fn extract_zip(zip_path: &Path, target: &Path) -> Result<(), DomainError> {
    let file = std::fs::File::open(zip_path)
        .map_err(|e| DomainError::Validation(format!("打开插件包失败：{e}")))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| DomainError::Validation(format!("非法 zip 包：{e}")))?;
    // 探测顶层包裹目录：所有条目都以同一顶层目录开头则剥离
    let mut names: Vec<String> = Vec::new();
    for i in 0..archive.len() {
        let f = archive
            .by_index(i)
            .map_err(|e| DomainError::Validation(format!("读取包条目失败：{e}")))?;
        names.push(f.name().to_string());
    }
    let strip_prefix = common_top_dir(&names);

    for i in 0..archive.len() {
        let mut f = archive
            .by_index(i)
            .map_err(|e| DomainError::Validation(format!("读取包条目失败：{e}")))?;
        let raw_name = f.name().to_string();
        let rel = match &strip_prefix {
            Some(prefix) => raw_name
                .strip_prefix(prefix)
                .map(str::to_string)
                .unwrap_or(raw_name),
            None => raw_name,
        };
        if rel.is_empty() || rel.ends_with('/') {
            continue;
        }
        // 防 zip-slip：拒绝 ../ 与绝对路径
        let rel_path = Path::new(&rel);
        if rel_path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir | std::path::Component::RootDir))
        {
            return Err(DomainError::Denied(format!("非法包内路径：{rel}")));
        }
        let out = target.join(rel_path);
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| DomainError::Validation(format!("创建目录失败：{e}")))?;
        }
        let mut w = std::fs::File::create(&out)
            .map_err(|e| DomainError::Validation(format!("写入文件失败：{e}")))?;
        std::io::copy(&mut f, &mut w)
            .map_err(|e| DomainError::Validation(format!("解压文件失败：{e}")))?;
        // 保留 zip 条目的 unix 权限位：sidecar 二进制（如 ./cyancatd）丢失可执行位会 spawn EACCES
        #[cfg(unix)]
        if let Some(mode) = f.unix_mode().filter(|m| *m != 0) {
            use std::os::unix::fs::PermissionsExt;
            let _ = w.set_permissions(std::fs::Permissions::from_mode(mode & 0o777));
        }
    }
    Ok(())
}

/// 若所有条目共享同一顶层目录，返回该前缀（含尾部 `/`）
fn common_top_dir(names: &[String]) -> Option<String> {
    let mut first_components = names
        .iter()
        .filter_map(|n| n.split('/').next().filter(|c| !c.is_empty()));
    let first = first_components.next()?;
    if first_components.all(|c| c == first) && names.iter().all(|n| n.contains('/')) {
        Some(format!("{first}/"))
    } else {
        None
    }
}

/// 解析包内 mcp.json（可选；缺失返回空）
pub fn read_mcp_decls(plugin_dir: &Path) -> Result<Vec<McpDecl>, DomainError> {
    let path = plugin_dir.join("mcp.json");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(&path)
        .map_err(|e| DomainError::Validation(format!("读取 mcp.json 失败：{e}")))?;
    serde_json::from_str(&text)
        .map_err(|e| DomainError::Validation(format!("mcp.json 格式非法：{e}")))
}

/// 解析包内 rules.json（可选；缺失返回空）
pub fn read_rule_decls(plugin_dir: &Path) -> Result<Vec<RuleDecl>, DomainError> {
    let path = plugin_dir.join("rules.json");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(&path)
        .map_err(|e| DomainError::Validation(format!("读取 rules.json 失败：{e}")))?;
    serde_json::from_str(&text)
        .map_err(|e| DomainError::Validation(format!("rules.json 格式非法：{e}")))
}

/// 读取已安装插件的 manifest
pub fn read_installed_manifest(plugin_dir: &Path) -> Result<PluginManifest, DomainError> {
    let text = std::fs::read_to_string(plugin_dir.join("manifest.json"))
        .map_err(|e| DomainError::Validation(format!("读取已安装插件 manifest 失败：{e}")))?;
    parse_manifest(&text)
}

/// 统计插件技能数（skills/*.md）
pub fn count_skills(plugin_dir: &Path) -> i64 {
    let dir = plugin_dir.join("skills");
    std::fs::read_dir(&dir)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("md"))
                .count() as i64
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_pkg(dir: &Path) {
        std::fs::create_dir_all(dir.join("skills")).unwrap();
        std::fs::write(
            dir.join("manifest.json"),
            r#"{"name":"my-plugin","version":"0.1.0","author":"a","description":"d","permissions":["skills","mcp","rules"]}"#,
        )
        .unwrap();
        std::fs::write(dir.join("skills/s1.md"), "---\nname: S1\n---\nbody").unwrap();
        std::fs::write(dir.join("mcp.json"), r#"[{"name":"fs","command":"npx mcp-fs"}]"#).unwrap();
        std::fs::write(
            dir.join("rules.json"),
            r#"[{"tool":"Bash","pattern":"cargo *","action":"allow","sort":1}]"#,
        )
        .unwrap();
    }

    #[test]
    fn install_from_directory() {
        let src = tempfile::tempdir().unwrap();
        write_pkg(src.path());
        let plugins = tempfile::tempdir().unwrap();

        let manifest = read_manifest_from_source(src.path()).unwrap();
        assert_eq!(manifest.name, "my-plugin");
        let target = extract_package(src.path(), plugins.path(), &manifest.name).unwrap();
        assert!(target.join("manifest.json").exists());
        assert!(target.join("skills/s1.md").exists());
        assert_eq!(count_skills(&target), 1);
        assert_eq!(read_mcp_decls(&target).unwrap()[0].name, "fs");
        assert_eq!(read_rule_decls(&target).unwrap()[0].pattern, "cargo *");

        // 重名冲突
        let err = extract_package(src.path(), plugins.path(), "my-plugin").unwrap_err();
        assert!(matches!(err, DomainError::Conflict(_)));
    }

    #[test]
    fn install_from_zip_with_top_dir_wrapper() {
        let src = tempfile::tempdir().unwrap();
        write_pkg(&src.path().join("pkg"));
        let zip_path = src.path().join("my-plugin.zip");
        // 打包时顶层包一层 my-plugin/
        let file = std::fs::File::create(&zip_path).unwrap();
        let mut zw = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default();
        zw.start_file("my-plugin/manifest.json", opts).unwrap();
        std::io::Write::write_all(
            &mut zw,
            br#"{"name":"my-plugin","version":"0.1.0","author":"a","description":"d","permissions":["skills"]}"#,
        )
        .unwrap();
        zw.start_file("my-plugin/skills/s1.md", opts).unwrap();
        std::io::Write::write_all(&mut zw, b"---\nname: S1\n---\nbody").unwrap();
        zw.finish().unwrap();

        let plugins = tempfile::tempdir().unwrap();
        let manifest = read_manifest_from_source(&zip_path).unwrap();
        let target = extract_package(&zip_path, plugins.path(), &manifest.name).unwrap();
        // 顶层包裹被剥离
        assert!(target.join("manifest.json").exists());
        assert!(target.join("skills/s1.md").exists());
    }

    #[test]
    fn zip_slip_rejected() {
        let src = tempfile::tempdir().unwrap();
        let zip_path = src.path().join("evil.zip");
        let file = std::fs::File::create(&zip_path).unwrap();
        let mut zw = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default();
        zw.start_file("evil/manifest.json", opts).unwrap();
        std::io::Write::write_all(&mut zw, br#"{"name":"evil"}"#).unwrap();
        zw.start_file("evil/../../escape.txt", opts).unwrap();
        std::io::Write::write_all(&mut zw, b"x").unwrap();
        zw.finish().unwrap();

        let plugins = tempfile::tempdir().unwrap();
        let manifest = read_manifest_from_source(&zip_path).unwrap();
        let err = extract_package(&zip_path, plugins.path(), &manifest.name).unwrap_err();
        assert!(matches!(err, DomainError::Denied(_)));
        // 失败回滚：不留半成品目录
        assert!(!plugins.path().join("evil").exists());
    }

    #[test]
    #[cfg(unix)]
    fn zip_preserves_exec_permission() {
        use std::os::unix::fs::PermissionsExt;

        let src = tempfile::tempdir().unwrap();
        let zip_path = src.path().join("bin-plugin.zip");
        let file = std::fs::File::create(&zip_path).unwrap();
        let mut zw = zip::ZipWriter::new(file);
        // 带 755 权限的二进制条目（sidecar 场景：backend.command = ./server）
        let opts = zip::write::SimpleFileOptions::default().unix_permissions(0o755);
        zw.start_file("bin-plugin/manifest.json", opts).unwrap();
        std::io::Write::write_all(
            &mut zw,
            br#"{"name":"bin-plugin","version":"1.0.0","author":"a","description":"d","permissions":["backend"]}"#,
        )
        .unwrap();
        zw.start_file("bin-plugin/server", opts).unwrap();
        std::io::Write::write_all(&mut zw, b"#!/bin/sh\nsleep 30\n").unwrap();
        zw.finish().unwrap();

        let plugins = tempfile::tempdir().unwrap();
        let manifest = read_manifest_from_source(&zip_path).unwrap();
        let target = extract_package(&zip_path, plugins.path(), &manifest.name).unwrap();
        // 解压后可执行位保留（丢失则 sidecar spawn 报 os error 13）
        let mode = std::fs::metadata(target.join("server"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o755);
    }

    #[test]
    fn manifest_missing_or_invalid() {
        let src = tempfile::tempdir().unwrap();
        assert!(read_manifest_from_source(src.path()).is_err());
        std::fs::write(src.path().join("manifest.json"), "{}").unwrap();
        assert!(read_manifest_from_source(src.path()).is_err(), "缺 name 应校验失败");
    }

    #[test]
    fn manifest_backend_section_parsed() {
        // 与 plugin_service 测试构造的 backend manifest 同款 JSON（含 frontendUrl）
        let command = "false";
        let health = ",\"healthPath\":\"/health\"";
        let text = format!(
            r#"{{"name":"backend-plugin","version":"1.0.0","author":"","description":"d","permissions":["backend","rules"],"backend":{{"command":"{command}"{health},"mcp":{{"name":"bp-mcp","url":"http://127.0.0.1:{{port}}/sse"}},"frontendUrl":"http://127.0.0.1:{{port}}/"}}}}"#
        );
        let m = parse_manifest(&text).unwrap();
        let backend = m.backend.expect("backend 段应解析成功");
        assert_eq!(backend.command, "false");
        assert_eq!(backend.health_path.as_deref(), Some("/health"));
        let mcp = backend.mcp.expect("mcp 声明应解析成功");
        assert_eq!(mcp.name, "bp-mcp");
        assert_eq!(mcp.url, "http://127.0.0.1:{port}/sse");
        assert_eq!(
            backend.frontend_url.as_deref(),
            Some("http://127.0.0.1:{port}/")
        );
    }
}
