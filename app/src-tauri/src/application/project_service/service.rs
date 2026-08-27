//! ProjectService 实现。

use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::project::{Project, ProjectRepository, ProjectTemplate};
use crate::error::ServiceError;
use crate::infra::db::now_local;
use crate::infra::{fs, git};

use super::{
    CreateProjectCmd, FileNodeBO, FilePreviewBO, FilePreviewQuery, FileTreeQuery, OpenProjectCmd,
    ProjectBO,
};

/// 项目服务
#[async_trait]
pub trait ProjectService: Send + Sync {
    /// 最近项目列表
    async fn list_projects(&self) -> Result<Vec<ProjectBO>, ServiceError>;
    /// 指定文件夹为项目（注册 + 记录打开时间）
    async fn open_project(&self, cmd: OpenProjectCmd) -> Result<ProjectBO, ServiceError>;
    /// 新建项目（脚手架）
    async fn create_project(&self, cmd: CreateProjectCmd) -> Result<ProjectBO, ServiceError>;
    /// 文件树
    async fn file_tree(&self, query: FileTreeQuery) -> Result<Vec<FileNodeBO>, ServiceError>;
    /// 文件预览（≤64KB）
    async fn file_preview(&self, query: FilePreviewQuery) -> Result<FilePreviewBO, ServiceError>;
}

/// 项目服务实现
pub struct ProjectServiceImpl {
    project_repo: Arc<dyn ProjectRepository>,
}

impl ProjectServiceImpl {
    /// 构造
    pub fn new(project_repo: Arc<dyn ProjectRepository>) -> Self {
        Self { project_repo }
    }
}

#[async_trait]
impl ProjectService for ProjectServiceImpl {
    async fn list_projects(&self) -> Result<Vec<ProjectBO>, ServiceError> {
        let projects = self.project_repo.list_recent(20).await?;
        Ok(projects.into_iter().map(ProjectBO::from).collect())
    }

    async fn open_project(&self, cmd: OpenProjectCmd) -> Result<ProjectBO, ServiceError> {
        let root = Project::validate_path(&cmd.path)?;
        let canonical = root.root().to_string_lossy().into_owned();
        if let Some(existing) = self.project_repo.find_by_path(&canonical).await? {
            self.project_repo.touch_last_opened(existing.id).await?;
            let mut bo = ProjectBO::from(existing);
            bo.last_opened_at = Some(now_local());
            return Ok(bo);
        }
        let mut project = Project::from_path(&root, now_local());
        self.project_repo.insert(&mut project).await?;
        Ok(ProjectBO::from(project))
    }

    async fn create_project(&self, cmd: CreateProjectCmd) -> Result<ProjectBO, ServiceError> {
        Project::validate_new_name(&cmd.name)?;
        let template = ProjectTemplate::parse(&cmd.template)?;
        let parent = Project::validate_path(&cmd.parent)?;
        let dir = parent.root().join(cmd.name.trim());
        if dir.exists() {
            return Err(ServiceError::conflict(format!(
                "目录已存在：{}",
                dir.display()
            )));
        }
        std::fs::create_dir_all(&dir)
            .map_err(|e| ServiceError::external(format!("创建项目目录失败：{e}")))?;
        // 脚手架：domain 产出文件清单，infra 执行 IO
        fs::write_scaffold(&dir, &template.scaffold_files(cmd.name.trim()))?;
        if cmd.git_init {
            git::ensure_repo(&dir)?;
        }
        let root = Project::validate_path(&dir.to_string_lossy())?;
        let mut project = Project::from_path(&root, now_local());
        self.project_repo.insert(&mut project).await?;
        Ok(ProjectBO::from(project))
    }

    async fn file_tree(&self, query: FileTreeQuery) -> Result<Vec<FileNodeBO>, ServiceError> {
        let root = Project::validate_path(&query.project_path)?;
        let nodes = fs::list_file_tree(&root)?;
        Ok(nodes.into_iter().map(FileNodeBO::from).collect())
    }

    async fn file_preview(&self, query: FilePreviewQuery) -> Result<FilePreviewBO, ServiceError> {
        let root = Project::validate_path(&query.project_path)?;
        let (content, truncated) = fs::preview_file(&root, &query.rel_path)?;
        Ok(FilePreviewBO { content, truncated })
    }
}
