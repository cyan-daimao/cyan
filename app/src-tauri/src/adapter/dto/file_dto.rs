//! 文件相关 Request / DTO。

use serde::{Deserialize, Serialize};

use crate::application::project_service::{
    FileNodeBO, FilePreviewBO, FilePreviewQuery, FileTreeQuery,
};

/// file_tree 请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileTreeRequest {
    /// 项目路径
    pub project_path: String,
}

impl From<FileTreeRequest> for FileTreeQuery {
    fn from(r: FileTreeRequest) -> Self {
        Self {
            project_path: r.project_path,
        }
    }
}

/// file_preview 请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilePreviewRequest {
    /// 项目路径
    pub project_path: String,
    /// 相对项目根路径
    pub rel_path: String,
}

impl From<FilePreviewRequest> for FilePreviewQuery {
    fn from(r: FilePreviewRequest) -> Self {
        Self {
            project_path: r.project_path,
            rel_path: r.rel_path,
        }
    }
}

/// 文件树节点 DTO
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileNodeDTO {
    /// 文件/目录名
    pub name: String,
    /// 相对项目根路径
    pub path: String,
    /// 是否目录
    pub is_dir: bool,
    /// 子节点
    pub children: Vec<FileNodeDTO>,
}

impl From<FileNodeBO> for FileNodeDTO {
    fn from(bo: FileNodeBO) -> Self {
        Self {
            name: bo.name,
            path: bo.path,
            is_dir: bo.is_dir,
            children: bo.children.into_iter().map(FileNodeDTO::from).collect(),
        }
    }
}

/// 文件预览 DTO
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilePreviewDTO {
    /// 内容（≤64KB）
    pub content: String,
    /// 是否截断
    pub truncated: bool,
}

impl From<FilePreviewBO> for FilePreviewDTO {
    fn from(bo: FilePreviewBO) -> Self {
        Self {
            content: bo.content,
            truncated: bo.truncated,
        }
    }
}
