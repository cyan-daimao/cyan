//! 项目业务对象。

use chrono::NaiveDateTime;

use crate::domain::project::Project;
use crate::infra::fs::FsNode;

/// 项目 BO
#[derive(Debug, Clone)]
pub struct ProjectBO {
    /// 项目 id
    pub id: i64,
    /// 项目名
    pub name: String,
    /// 绝对路径
    pub path: String,
    /// 最近打开时间
    pub last_opened_at: Option<NaiveDateTime>,
}

impl From<Project> for ProjectBO {
    fn from(p: Project) -> Self {
        Self {
            id: p.id,
            name: p.name,
            path: p.path,
            last_opened_at: p.last_opened_at,
        }
    }
}

/// 文件树节点 BO
#[derive(Debug, Clone)]
pub struct FileNodeBO {
    /// 文件/目录名
    pub name: String,
    /// 相对项目根路径
    pub path: String,
    /// 是否目录
    pub is_dir: bool,
    /// 子节点
    pub children: Vec<FileNodeBO>,
}

impl From<FsNode> for FileNodeBO {
    fn from(n: FsNode) -> Self {
        Self {
            name: n.name,
            path: n.rel_path,
            is_dir: n.is_dir,
            children: n.children.into_iter().map(FileNodeBO::from).collect(),
        }
    }
}

/// 文件预览 BO
#[derive(Debug, Clone)]
pub struct FilePreviewBO {
    /// 内容（≤64KB）
    pub content: String,
    /// 是否被截断
    pub truncated: bool,
}
