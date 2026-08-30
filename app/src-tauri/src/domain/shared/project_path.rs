//! ProjectPath：项目根路径值对象，统一做 canonicalize 前缀校验，防 `../` 逃逸与绝对路径越权。
//! 另提供只读白名单解析（`resolve_readonly`）：项目外仅放行显式登记的宿主目录（如 `~/.cyan`）。

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

    /// 只读解析：项目内正常放行；项目外仅当落在 `extra_roots` 白名单内才放行
    /// （用于读取 `~/.cyan` 宿主配置目录：技能/插件文件/日志等；调用方保证只读不写）。
    /// 支持 `~` 开头路径（先展开）。写路径仍走 [`Self::resolve`]，不受白名单影响。
    pub fn resolve_readonly(&self, rel: &str, extra_roots: &[PathBuf]) -> Result<PathBuf, DomainError> {
        let expanded = expand_tilde(Path::new(rel));
        let rel_text = expanded.to_string_lossy().into_owned();
        match self.resolve(&rel_text) {
            Ok(p) => Ok(p),
            Err(DomainError::Denied(_)) => {
                let rel_path = Path::new(&rel_text);
                for extra in extra_roots {
                    // 白名单根先 canonicalize 再比较，防软链绕过前缀校验
                    let extra_canon = Self::canonicalize_lenient(extra)?;
                    let joined = if rel_path.is_absolute() {
                        rel_path.to_path_buf()
                    } else {
                        extra_canon.join(rel_path)
                    };
                    let canonical = Self::canonicalize_lenient(&joined)?;
                    if canonical.starts_with(&extra_canon) {
                        return Ok(canonical);
                    }
                }
                Err(DomainError::Denied(format!(
                    "路径越权（不在项目根或只读白名单内）：{rel}"
                )))
            }
            Err(e) => Err(e),
        }
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
    fn readonly_resolves_project_paths_and_tilde() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();
        std::fs::write(tmp.path().join("src/a.rs"), "fn main() {}").unwrap();
        let root = ProjectPath::new(tmp.path()).unwrap();
        // 项目内路径行为与 resolve 一致
        let p = root
            .resolve_readonly("src/a.rs", &[PathBuf::from("/nonexistent-allowlist")])
            .unwrap();
        assert!(p.starts_with(root.root()));
        // `~` 展开后落在项目内（测试项目在 home 下时同样放行）
        let tilde_in_project = format!("{}/src", tmp.path().display());
        let p = root.resolve_readonly(&tilde_in_project, &[]).unwrap();
        assert!(p.starts_with(root.root()));
    }

    #[test]
    fn readonly_allows_whitelisted_root_only() {
        let tmp = tempfile::tempdir().unwrap();
        let allow = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(allow.path().join("plugins/cyancat")).unwrap();
        std::fs::write(allow.path().join("plugins/cyancat/log.txt"), "log line").unwrap();
        let root = ProjectPath::new(tmp.path()).unwrap();
        let allow_root = allow.path().to_path_buf();

        // 白名单内：绝对路径放行（macOS 下 /var → /private/var 符号链接，与 canonicalize 后比较）
        let abs = allow.path().join("plugins/cyancat/log.txt");
        let p = root
            .resolve_readonly(&abs.to_string_lossy(), &[allow_root.clone()])
            .unwrap();
        assert_eq!(p, abs.canonicalize().unwrap());
        // 相对路径是项目优先语义：项目内不存在的相对路径按新文件解析进项目（白名单只对逃逸路径生效）
        let p = root
            .resolve_readonly("plugins/cyancat/log.txt", &[allow_root.clone()])
            .unwrap();
        assert!(p.starts_with(root.root()));

        // 白名单外：拒绝
        let err = root.resolve_readonly("/etc/passwd", &[allow_root]).unwrap_err();
        assert!(matches!(err, DomainError::Denied(_)));
        // 空白名单等价于 resolve
        let err = root.resolve_readonly("../outside.txt", &[]).unwrap_err();
        assert!(matches!(err, DomainError::Denied(_)));
        // 白名单根不存在时：相对路径项目优先解析（新文件语义），与 resolve 一致
        let p = root
            .resolve_readonly("x.txt", &[PathBuf::from("/nonexistent-allowlist")])
            .unwrap();
        assert_eq!(p, root.root().join("x.txt"));
    }

    #[test]
    fn readonly_escape_within_allowlist_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let allow = tempfile::tempdir().unwrap();
        let root = ProjectPath::new(tmp.path()).unwrap();
        // 从白名单内再逃逸出去：拒绝
        let err = root
            .resolve_readonly("../outside.txt", &[allow.path().to_path_buf()])
            .unwrap_err();
        assert!(matches!(err, DomainError::Denied(_)));
        // 白名单子目录逃逸（绝对路径写法，直接命中白名单分支）：拒绝
        let nested = allow.path().join("sub");
        std::fs::create_dir_all(&nested).unwrap();
        let abs_escape = format!("{}/../../evil.txt", nested.display());
        let err = root
            .resolve_readonly(&abs_escape, &[allow.path().to_path_buf()])
            .unwrap_err();
        assert!(matches!(err, DomainError::Denied(_)));
    }

    #[test]
    fn new_rejects_missing_path() {
        let err = ProjectPath::new("/definitely/not/exist/cyan-test").unwrap_err();
        assert!(matches!(err, DomainError::NotFound(_)));
    }
}
