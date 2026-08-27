//! 文件系统：文件树读取（忽略规则 + 深度限制）、文件预览（≤64KB 截断、二进制拒绝）、脚手架写入。

use std::path::Path;

use crate::domain::shared::ProjectPath;
use crate::domain::DomainError;

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

/// 文件预览：逃逸校验 + 二进制拒绝 + ≤64KB 截断，返回 (内容, 是否截断)
pub fn preview_file(root: &ProjectPath, rel_path: &str) -> Result<(String, bool), DomainError> {
    let abs = root.resolve(rel_path)?;
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

/// 读取文件全文（工具执行用，限制 256KB 防内存爆炸）
pub fn read_text_file(root: &ProjectPath, rel_path: &str) -> Result<String, DomainError> {
    const READ_LIMIT: usize = 256 * 1024;
    let abs = root.resolve(rel_path)?;
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
}
