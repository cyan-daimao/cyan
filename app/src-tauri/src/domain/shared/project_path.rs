//! ProjectPath：项目根路径值对象，统一做 canonicalize 前缀校验，防 `../` 逃逸与绝对路径越权。

use std::path::{Path, PathBuf};

use crate::domain::DomainError;

/// 项目根路径（已 canonicalize），所有文件/命令路径解析的唯一入口
#[derive(Debug, Clone)]
pub struct ProjectPath {
    /// canonicalize 后的项目根绝对路径
    root: PathBuf,
}

impl ProjectPath {
    /// 校验路径存在并 canonicalize 为项目根；开头 `~` 先展开为用户主目录
    pub fn new(path: impl AsRef<Path>) -> Result<Self, DomainError> {
        let expanded = expand_tilde(path.as_ref());
        let path = expanded.as_path();
        if !path.exists() {
            return Err(DomainError::NotFound(format!(
                "项目路径不存在：{}",
                path.display()
            )));
        }
        if !path.is_dir() {
            return Err(DomainError::Validation(format!(
                "项目路径不是目录：{}",
                path.display()
            )));
        }
        let root = path
            .canonicalize()
            .map_err(|e| DomainError::Validation(format!("路径 canonicalize 失败：{e}")))?;
        Ok(Self { root })
    }

    /// 项目根绝对路径
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// 将相对路径解析为项目内绝对路径；解析结果必须仍以项目根为前缀，否则拒绝。
    /// 支持目标尚不存在的路径（写新文件）：对最近的已存在祖先做 canonicalize。
    pub fn resolve(&self, rel: &str) -> Result<PathBuf, DomainError> {
        if rel.trim().is_empty() {
            return Err(DomainError::Validation("相对路径不能为空".into()));
        }
        let rel_path = Path::new(rel);
        // 绝对路径直接按越权候选处理，统一走 canonicalize + 前缀校验
        let joined = if rel_path.is_absolute() {
            rel_path.to_path_buf()
        } else {
            self.root.join(rel_path)
        };
        let canonical = Self::canonicalize_lenient(&joined)?;
        if !canonical.starts_with(&self.root) {
            return Err(DomainError::Denied(format!(
                "路径越权（逃逸项目根）：{rel}"
            )));
        }
        Ok(canonical)
    }

    /// 宽松 canonicalize：路径不存在时，canonicalize 其最近的已存在祖先再拼接剩余部分
    fn canonicalize_lenient(path: &Path) -> Result<PathBuf, DomainError> {
        if let Ok(p) = path.canonicalize() {
            return Ok(p);
        }
        // 逐级向上找已存在的祖先
        let mut missing: Vec<std::ffi::OsString> = Vec::new();
        let mut cursor = path.to_path_buf();
        loop {
            let file_name = cursor
                .file_name()
                .ok_or_else(|| DomainError::Validation(format!("非法路径：{}", path.display())))?;
            missing.push(file_name.to_os_string());
            if !cursor.pop() {
                return Err(DomainError::Validation(format!(
                    "非法路径：{}",
                    path.display()
                )));
            }
            if cursor.exists() {
                let mut base = cursor.canonicalize().map_err(|e| {
                    DomainError::Validation(format!("路径 canonicalize 失败：{e}"))
                })?;
                for seg in missing.iter().rev() {
                    base.push(seg);
                }
                return Ok(base);
            }
        }
    }

    /// 将项目内绝对路径转回相对路径（用于展示与落库）
    pub fn relativize(&self, abs: &Path) -> String {
        abs.strip_prefix(&self.root)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| abs.to_string_lossy().into_owned())
    }
}

/// 展开路径开头的 `~` 为用户主目录（`~`、`~/x`），其余原样返回；不依赖第三方 crate
fn expand_tilde(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if s == "~" || s.starts_with("~/") || s.starts_with("~\\") {
        if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
            let mut p = PathBuf::from(home);
            let rest = s[1..].trim_start_matches(['/', '\\']);
            if !rest.is_empty() {
                p.push(rest);
            }
            return p;
        }
    }
    path.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_tilde_home() {
        let home = std::env::var_os("HOME").expect("tests need HOME");
        let expanded = expand_tilde(Path::new("~/some/dir"));
        assert_eq!(expanded, PathBuf::from(home).join("some/dir"));
    }

    #[test]
    fn expand_tilde_keeps_normal_path() {
        let p = expand_tilde(Path::new("/tmp/cyan-x"));
        assert_eq!(p, PathBuf::from("/tmp/cyan-x"));
    }

    #[test]
    fn resolve_normal_rel_path() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();
        std::fs::write(tmp.path().join("src/a.rs"), "fn main() {}").unwrap();
        let root = ProjectPath::new(tmp.path()).unwrap();
        let p = root.resolve("src/a.rs").unwrap();
        assert!(p.ends_with("src/a.rs"));
        assert!(p.starts_with(root.root()));
    }

    #[test]
    fn resolve_new_file_under_existing_dir() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();
        let root = ProjectPath::new(tmp.path()).unwrap();
        let p = root.resolve("src/new.rs").unwrap();
        assert!(p.starts_with(root.root()));
    }

    #[test]
    fn reject_dotdot_escape() {
        let tmp = tempfile::tempdir().unwrap();
        let root = ProjectPath::new(tmp.path()).unwrap();
        let err = root.resolve("../outside.txt").unwrap_err();
        assert!(matches!(err, DomainError::Denied(_)));
    }

    #[test]
    fn reject_absolute_path_outside_root() {
        let tmp = tempfile::tempdir().unwrap();
        let root = ProjectPath::new(tmp.path()).unwrap();
        let err = root.resolve("/etc/passwd").unwrap_err();
        assert!(matches!(err, DomainError::Denied(_)));
    }

    #[test]
    fn reject_nested_dotdot_escape() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("sub")).unwrap();
        let root = ProjectPath::new(tmp.path()).unwrap();
        // 先进入子目录再逃逸出项目根
        let err = root.resolve("sub/../../outside.txt").unwrap_err();
        assert!(matches!(err, DomainError::Denied(_)));
    }

    #[test]
    fn new_rejects_missing_path() {
        let err = ProjectPath::new("/definitely/not/exist/cyan-test").unwrap_err();
        assert!(matches!(err, DomainError::NotFound(_)));
    }
}
