//! 项目相关 Request / DTO。

use serde::{Deserialize, Serialize};

use crate::application::project_service::{
    CreateProjectCmd, OpenProjectCmd, ProjectBO, RemoveProjectCmd,
};
use crate::infra::db::fmt_time;

/// open_project 请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenProjectRequest {
    /// 项目目录路径
    pub path: String,
}

impl From<OpenProjectRequest> for OpenProjectCmd {
    fn from(r: OpenProjectRequest) -> Self {
        Self { path: r.path }
    }
}

/// create_project 请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProjectRequest {
    /// 项目名
    pub name: String,
    /// 父目录
    pub parent: String,
    /// 模板（empty/rust/node）
    pub template: String,
    /// 是否初始化 git
    pub git_init: bool,
}

impl From<CreateProjectRequest> for CreateProjectCmd {
    fn from(r: CreateProjectRequest) -> Self {
        Self {
            name: r.name,
            parent: r.parent,
            template: r.template,
            git_init: r.git_init,
        }
    }
}

/// remove_project 请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveProjectRequest {
    /// 项目目录路径
    pub path: String,
}

impl From<RemoveProjectRequest> for RemoveProjectCmd {
    fn from(r: RemoveProjectRequest) -> Self {
        Self { path: r.path }
    }
}

/// 项目 DTO
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDTO {
    /// 项目 id
    pub id: i64,
    /// 项目名
    pub name: String,
    /// 绝对路径
    pub path: String,
    /// 最近打开时间
    pub last_opened_at: Option<String>,
}

impl From<ProjectBO> for ProjectDTO {
    fn from(bo: ProjectBO) -> Self {
        Self {
            id: bo.id,
            name: bo.name,
            path: bo.path,
            last_opened_at: bo.last_opened_at.as_ref().map(fmt_time),
        }
    }
}
