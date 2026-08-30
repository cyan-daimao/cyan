//! 文件系统：文件树读取（忽略规则 + 深度限制）、文件预览（≤64KB 截断、二进制拒绝）、脚手架写入、
//! Glob/Grep 只读搜索、AGENTS.md 项目指令读取。

use std::path::{Path, PathBuf};

use crate::domain::shared::ProjectPath;
use crate::domain::DomainError;

pub mod skill;

/// Agent 只读白名单：`~/.cyan` 宿主目录（技能/插件文件/日志等）。
/// Read/Grep/Glob 对其中文件的读取放行；写入类工具不走白名单（仍限项目内）。
fn readonly_allowlist() -> Vec<PathBuf> {
    match crate::infra::db::datasource::cyan_home() {
        Ok(home) => vec![home],
        Err(_) => Vec::new(),
    }
}

/// 单文件预览上限（64KB，TECH_DESIGN 第 7 章）
pub const PREVIEW_LIMIT: usize = 64 * 1024;

/// 文件树最大递归深度
const TREE_MAX_DEPTH: usize = 4;

/// 文件树忽略的目录/文件名
const TREE_IGNORE: &[&str] = &[
    ".git", "node_modules", "target", "dist", ".DS_Store", ".idea", ".vscode",
];

/// 文件树节点（infra 传输结构，application 转 BO）
#[derive(Debug, Clone)]
pub struct FsNode {
    /// 文件/目录名
    pub name: String,
    /// 相对项目根路径
    pub rel_path: String,
    /// 是否目录
    pub is_dir: bool,
    /// 子节点（目录时存在）
    pub children: Vec<FsNode>,
}

/// 读取文件树：目录优先、名称升序；忽略 .git/node_modules/target 等，深度 ≤ 4
pub fn list_file_tree(root: &ProjectPath) -> Result<Vec<FsNode>, DomainError> {
    walk(root, root.root(), 0)
}

fn walk(root: &ProjectPath, dir: &Path, depth: usize) -> Result<Vec<FsNode>, DomainError> {
    if depth >= TREE_MAX_DEPTH {
        return Ok(Vec::new());
    }
    let entries = std::fs::read_dir(dir)
        .map_err(|e| DomainError::Validation(format!("读取目录失败 {}：{e}", dir.display())))?;
    let mut nodes: Vec<FsNode> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if TREE_IGNORE.contains(&name.as_str()) {
            continue;
        }
        let path = entry.path();
        let is_dir = path.is_dir();
        let rel_path = root.relativize(&path);
        let children = if is_dir {
            walk(root, &path, depth + 1).unwrap_or_default()
        } else {
            Vec::new()
        };
        nodes.push(FsNode {
            name,
            rel_path,
            is_dir,
            children,
        });
    }
    nodes.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.cmp(&b.name)));
    Ok(nodes)
}

/// 文件预览：只读逃逸校验（含 `~/.cyan` 白名单）+ 二进制拒绝 + ≤64KB 截断，返回 (内容, 是否截断)
pub fn preview_file(root: &ProjectPath, rel_path: &str) -> Result<(String, bool), DomainError> {
    let abs = root.resolve_readonly(rel_path, &readonly_allowlist())?;
    if abs.is_dir() {
        return Err(DomainError::Validation("目标是目录，无法预览".into()));
    }
    let bytes = std::fs::read(&abs)
        .map_err(|e| DomainError::Validation(format!("读取文件失败：{e}")))?;
    let truncated = bytes.len() > PREVIEW_LIMIT;
    let head = &bytes[..bytes.len().min(PREVIEW_LIMIT)];
    if head.contains(&0) {
        return Err(DomainError::Validation("二进制文件不支持预览".into()));
    }
    let content = String::from_utf8(head.to_vec())
        .map_err(|_| DomainError::Validation("文件不是有效 UTF-8 文本".into()))?;
    Ok((content, truncated))
}

/// 读取文件全文（工具执行用，限制 256KB 防内存爆炸；只读，允许 `~/.cyan` 白名单）
pub fn read_text_file(root: &ProjectPath, rel_path: &str) -> Result<String, DomainError> {
    const READ_LIMIT: usize = 256 * 1024;
    let abs = root.resolve_readonly(rel_path, &readonly_allowlist())?;
    let bytes =
        std::fs::read(&abs).map_err(|e| DomainError::Validation(format!("读取文件失败：{e}")))?;
    if bytes.len() > READ_LIMIT {
        return Err(DomainError::Validation("文件超过 256KB，不支持整体读取".into()));
    }
    String::from_utf8(bytes).map_err(|_| DomainError::Validation("文件不是有效 UTF-8 文本".into()))
}

/// 写文件（自动创建父目录）；调用前须已做逃逸校验
pub fn write_file_abs(abs: &Path, content: &str) -> Result<(), DomainError> {
    if let Some(parent) = abs.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| DomainError::Validation(format!("创建目录失败：{e}")))?;
    }
    std::fs::write(abs, content).map_err(|e| DomainError::Validation(format!("写文件失败：{e}")))
}

/// 写入脚手架文件清单（domain ProjectTemplate::scaffold_files 的输出）
pub fn write_scaffold(dir: &Path, files: &[(String, String)]) -> Result<(), DomainError> {
    for (rel, content) in files {
        let path = dir.join(rel);
        write_file_abs(&path, content)?;
    }
    Ok(())
}

/// Glob 匹配结果上限
const GLOB_LIMIT: usize = 500;

/// Grep 匹配结果上限
const GREP_LIMIT: usize = 200;

/// Grep 单文件大小上限（1MB，超出跳过）
const GREP_FILE_LIMIT: u64 = 1024 * 1024;

/// AGENTS.md 读取上限（8KB）
const AGENTS_MD_LIMIT: usize = 8 * 1024;

/// 解析可选子目录（相对项目根），缺省返回项目根；只读搜索（Glob/Grep）允许 `~/.cyan` 白名单内目录
fn resolve_base(root: &ProjectPath, path: Option<&str>) -> Result<std::path::PathBuf, DomainError> {
    match path {
        Some(p) if !p.trim().is_empty() => {
            let abs = root.resolve_readonly(p, &readonly_allowlist())?;
            if !abs.is_dir() {
                return Err(DomainError::Validation(format!("不是目录：{p}")));
            }
            Ok(abs)
        }
        _ => Ok(root.root().to_path_buf()),
    }
}

/// 递归收集文件（跳过 .git/node_modules/target 等），对相对路径调用 visit；返回是否提前停止
fn collect_files(root: &ProjectPath, dir: &Path, visit: &mut dyn FnMut(&str, &Path) -> bool) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if TREE_IGNORE.contains(&name.as_str()) {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, visit);
        } else {
            let rel = root.relativize(&path);
            // visit 返回 false 表示达到上限，停止遍历
            if !visit(&rel, &path) {
                return;
            }
        }
    }
}

/// Glob 搜索：返回匹配 pattern 的相对路径列表（按路径排序，上限 500 条）
pub fn glob_files(
    root: &ProjectPath,
    pattern: &str,
    path: Option<&str>,
) -> Result<Vec<String>, DomainError> {
    let matcher = globset::Glob::new(pattern)
        .map_err(|e| DomainError::Validation(format!("非法 glob 模式：{e}")))?
        .compile_matcher();
    let base = resolve_base(root, path)?;
    let mut hits: Vec<String> = Vec::new();
    collect_files(root, &base, &mut |rel, _| {
        if matcher.is_match(rel) {
            hits.push(rel.to_string());
        }
        hits.len() < GLOB_LIMIT
    });
    hits.sort();
    Ok(hits)
}

/// Grep 命中行
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrepHit {
    /// 相对项目根路径
    pub rel_path: String,
    /// 行号（从 1 开始）
    pub line_no: usize,
    /// 行内容
    pub line: String,
}

/// Grep 搜索：正则匹配行，输出命中列表（上限 200 条，跳过二进制与 >1MB 文件）。
/// `include` 为可选 glob（作用于相对路径，如 `*.rs`、`src/**/*.ts`）。
pub fn grep_files(
    root: &ProjectPath,
    pattern: &str,
    include: Option<&str>,
    path: Option<&str>,
) -> Result<Vec<GrepHit>, DomainError> {
    let re = regex::Regex::new(pattern)
        .map_err(|e| DomainError::Validation(format!("非法正则：{e}")))?;
    let include_matcher = include
        .filter(|i| !i.trim().is_empty())
        .map(|i| {
            globset::Glob::new(i)
                .map(|g| g.compile_matcher())
                .map_err(|e| DomainError::Validation(format!("非法 include 模式：{e}")))
        })
        .transpose()?;
    let base = resolve_base(root, path)?;
    let mut hits: Vec<GrepHit> = Vec::new();
    collect_files(root, &base, &mut |rel, abs| {
        if hits.len() >= GREP_LIMIT {
            return false;
        }
        if let Some(m) = &include_matcher {
            if !m.is_match(rel) {
                return true;
            }
        }
        // 跳过大文件
        let Ok(meta) = abs.metadata() else { return true };
        if meta.len() > GREP_FILE_LIMIT {
            return true;
        }
        let Ok(bytes) = std::fs::read(abs) else { return true };
        // 跳过二进制
        let head = &bytes[..bytes.len().min(8192)];
        if head.contains(&0) {
            return true;
        }
        let Ok(text) = String::from_utf8(bytes) else { return true };
        for (idx, line) in text.lines().enumerate() {
            if re.is_match(line) {
                hits.push(GrepHit {
                    rel_path: rel.to_string(),
                    line_no: idx + 1,
                    line: line.to_string(),
                });
                if hits.len() >= GREP_LIMIT {
                    return false;
                }
            }
        }
        true
    });
    Ok(hits)
}

/// 读取项目根的 AGENTS.md（截断 8KB）；不存在或非法时静默返回 None
pub fn read_agents_md(root: &ProjectPath) -> Option<String> {
    let abs = root.resolve("AGENTS.md").ok()?;
    if !abs.is_file() {
        return None;
    }
    let bytes = std::fs::read(&abs).ok()?;
    let head = &bytes[..bytes.len().min(AGENTS_MD_LIMIT)];
    if head.contains(&0) {
        return None;
    }
    String::from_utf8(head.to_vec()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_truncates_over_64kb() {
        let tmp = tempfile::tempdir().unwrap();
        let big = "a".repeat(PREVIEW_LIMIT + 100);
        std::fs::write(tmp.path().join("big.txt"), big).unwrap();
        let root = ProjectPath::new(tmp.path()).unwrap();
        let (content, truncated) = preview_file(&root, "big.txt").unwrap();
        assert!(truncated);
        assert_eq!(content.len(), PREVIEW_LIMIT);
    }

    #[test]
    fn preview_rejects_binary_and_escape() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("bin.dat"), [0u8, 159, 146, 150]).unwrap();
        let root = ProjectPath::new(tmp.path()).unwrap();
        assert!(preview_file(&root, "bin.dat").is_err());
        assert!(preview_file(&root, "../outside.txt").is_err());
    }

    #[test]
    fn tree_ignores_git_and_sorts_dirs_first() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join(".git")).unwrap();
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();
        std::fs::write(tmp.path().join("a.txt"), "x").unwrap();
        let root = ProjectPath::new(tmp.path()).unwrap();
        let nodes = list_file_tree(&root).unwrap();
        assert_eq!(nodes.len(), 2);
        assert!(nodes[0].is_dir);
        assert_eq!(nodes[0].name, "src");
        assert_eq!(nodes[1].name, "a.txt");
    }

    #[test]
    fn glob_filters_and_skips_ignored_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("src/agent")).unwrap();
        std::fs::create_dir_all(tmp.path().join("node_modules/pkg")).unwrap();
        std::fs::write(tmp.path().join("src/main.rs"), "").unwrap();
        std::fs::write(tmp.path().join("src/agent/mod.rs"), "").unwrap();
        std::fs::write(tmp.path().join("src/agent/readme.md"), "").unwrap();
        std::fs::write(tmp.path().join("node_modules/pkg/index.rs"), "").unwrap();
        let root = ProjectPath::new(tmp.path()).unwrap();

        let hits = glob_files(&root, "src/**/*.rs", None).unwrap();
        assert_eq!(hits, vec!["src/agent/mod.rs", "src/main.rs"]);

        // path 子目录限定 + include 全量
        let hits = glob_files(&root, "**/*.md", Some("src")).unwrap();
        assert_eq!(hits, vec!["src/agent/readme.md"]);

        // 非法 pattern 报错
        assert!(glob_files(&root, "[", None).is_err());
    }

    #[test]
    fn grep_matches_with_include_and_skips_binary() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();
        std::fs::write(tmp.path().join("src/a.rs"), "fn main() {}\n// todo: fix\n").unwrap();
        std::fs::write(tmp.path().join("src/b.md"), "todo here\n").unwrap();
        std::fs::write(tmp.path().join("src/c.bin"), [0u8, 1, 2]).unwrap();
        let root = ProjectPath::new(tmp.path()).unwrap();

        let hits = grep_files(&root, "todo", None, None).unwrap();
        assert_eq!(hits.len(), 2);
        assert!(hits.iter().any(|h| h.rel_path == "src/a.rs" && h.line_no == 2));
        assert!(hits.iter().all(|h| h.rel_path != "src/c.bin"), "二进制应跳过");

        // include glob 过滤
        let hits = grep_files(&root, "todo", Some("*.rs"), None).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].rel_path, "src/a.rs");

        // 非法正则报错
        assert!(grep_files(&root, "(", None, None).is_err());
    }

    #[test]
    fn read_agents_md_returns_none_when_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let root = ProjectPath::new(tmp.path()).unwrap();
        assert!(read_agents_md(&root).is_none());
        std::fs::write(tmp.path().join("AGENTS.md"), "项目规则\n").unwrap();
        assert_eq!(read_agents_md(&root).as_deref(), Some("项目规则\n"));
    }
}
