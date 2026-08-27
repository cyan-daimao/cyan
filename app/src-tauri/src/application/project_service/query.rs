//! 项目/文件查询对象。

/// 文件树查询
#[derive(Debug, Clone)]
pub struct FileTreeQuery {
    /// 项目路径
    pub project_path: String,
}

/// 文件预览查询
#[derive(Debug, Clone)]
pub struct FilePreviewQuery {
    /// 项目路径
    pub project_path: String,
    /// 相对项目根路径
    pub rel_path: String,
}
