//! Project：项目充血对象（validate_path / scaffold 文件清单 / ensure_git 判定）。

use chrono::NaiveDateTime;

use crate::domain::shared::ProjectPath;
use crate::domain::DomainError;

/// 项目模板
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectTemplate {
    /// 空项目
    Empty,
    /// Rust（cargo 项目骨架）
    Rust,
    /// Node（npm 项目骨架）
    Node,
}

impl ProjectTemplate {
    /// 从请求字符串解析
    pub fn parse(s: &str) -> Result<Self, DomainError> {
        match s {
            "empty" => Ok(Self::Empty),
            "rust" => Ok(Self::Rust),
            "node" => Ok(Self::Node),
            other => Err(DomainError::Validation(format!("未知项目模板：{other}"))),
        }
    }

    /// 脚手架文件清单（相对路径 → 内容），IO 由 infra/fs 执行
    pub fn scaffold_files(&self, project_name: &str) -> Vec<(String, String)> {
        match self {
            Self::Empty => vec![(
                "README.md".to_string(),
                format!("# {project_name}\n"),
            )],
            Self::Rust => vec![
                (
                    "Cargo.toml".to_string(),
                    format!(
                        "[package]\nname = \"{project_name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"
                    ),
                ),
                (
                    "src/main.rs".to_string(),
                    "fn main() {\n    println!(\"Hello, world!\");\n}\n".to_string(),
                ),
                (".gitignore".to_string(), "/target\n".to_string()),
            ],
            Self::Node => vec![
                (
                    "package.json".to_string(),
                    format!(
                        "{{\n  \"name\": \"{project_name}\",\n  \"version\": \"0.1.0\",\n  \"private\": true\n}}\n"
                    ),
                ),
                ("index.js".to_string(), "console.log('Hello, world!');\n".to_string()),
                (".gitignore".to_string(), "node_modules/\n".to_string()),
            ],
        }
    }
}

/// 项目
#[derive(Debug, Clone)]
pub struct Project {
    /// 主键 id（插入后回填）
    pub id: i64,
    /// 项目名
    pub name: String,
    /// 绝对路径（canonicalize 后）
    pub path: String,
    /// 最近打开时间
    pub last_opened_at: Option<NaiveDateTime>,
    /// 创建时间
    pub created_at: NaiveDateTime,
    /// 更新时间
    pub updated_at: NaiveDateTime,
}

impl Project {
    /// 校验路径存在性与合法性，返回项目根值对象（canonicalize 后）
    pub fn validate_path(path: &str) -> Result<ProjectPath, DomainError> {
        ProjectPath::new(path)
    }

    /// 由项目根值对象构造项目（未持久化，id 待回填）
    pub fn from_path(root: &ProjectPath, now: NaiveDateTime) -> Self {
        let name = root
            .root()
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "未命名项目".to_string());
        Self {
            id: 0,
            name,
            path: root.root().to_string_lossy().into_owned(),
            last_opened_at: Some(now),
            created_at: now,
            updated_at: now,
        }
    }

    /// 校验新项目名（目录名合法）
    pub fn validate_new_name(name: &str) -> Result<(), DomainError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(DomainError::Validation("项目名不能为空".into()));
        }
        if name.contains('/') || name.contains('\\') || name.starts_with('.') {
            return Err(DomainError::Validation(
                "项目名不能包含路径分隔符或以 . 开头".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_parse() {
        assert_eq!(ProjectTemplate::parse("empty").unwrap(), ProjectTemplate::Empty);
        assert!(ProjectTemplate::parse("java").is_err());
    }

    #[test]
    fn scaffold_files_nonempty() {
        assert!(!ProjectTemplate::Empty.scaffold_files("demo").is_empty());
        assert!(ProjectTemplate::Rust
            .scaffold_files("demo")
            .iter()
            .any(|(p, _)| p == "Cargo.toml"));
    }

    #[test]
    fn validate_new_name_rules() {
        assert!(Project::validate_new_name("my-app").is_ok());
        assert!(Project::validate_new_name("  ").is_err());
        assert!(Project::validate_new_name("a/b").is_err());
        assert!(Project::validate_new_name(".hidden").is_err());
    }
}
