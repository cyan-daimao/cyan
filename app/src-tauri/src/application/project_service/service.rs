//! ProjectService 实现。

use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::agent::CheckpointRepository;
use crate::domain::config::PermRuleRepository;
use crate::domain::project::{Project, ProjectRepository, ProjectTemplate};
use crate::domain::session::{MessageRepository, SessionRepository};
use crate::error::ServiceError;
use crate::infra::db::{fmt_time, now_local};
use crate::infra::{fs, git};

use super::{
    CreateProjectCmd, FileNodeBO, FilePreviewBO, FilePreviewQuery, FileTreeQuery, OpenProjectCmd,
    ProjectBO, RemoveProjectCmd,
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
    /// 移除项目（软删记录 + 级联软删其下会话/消息/checkpoint/项目级规则，全部进回收站；幂等）
    async fn remove_project(&self, cmd: RemoveProjectCmd) -> Result<(), ServiceError>;
    /// 文件树
    async fn file_tree(&self, query: FileTreeQuery) -> Result<Vec<FileNodeBO>, ServiceError>;
    /// 文件预览（≤64KB）
    async fn file_preview(&self, query: FilePreviewQuery) -> Result<FilePreviewBO, ServiceError>;
}

/// 项目服务实现
pub struct ProjectServiceImpl {
    project_repo: Arc<dyn ProjectRepository>,
    session_repo: Arc<dyn SessionRepository>,
    message_repo: Arc<dyn MessageRepository>,
    checkpoint_repo: Arc<dyn CheckpointRepository>,
    perm_rule_repo: Arc<dyn PermRuleRepository>,
}

impl ProjectServiceImpl {
    /// 构造
    pub fn new(
        project_repo: Arc<dyn ProjectRepository>,
        session_repo: Arc<dyn SessionRepository>,
        message_repo: Arc<dyn MessageRepository>,
        checkpoint_repo: Arc<dyn CheckpointRepository>,
        perm_rule_repo: Arc<dyn PermRuleRepository>,
    ) -> Self {
        Self {
            project_repo,
            session_repo,
            message_repo,
            checkpoint_repo,
            perm_rule_repo,
        }
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

    async fn remove_project(&self, cmd: RemoveProjectCmd) -> Result<(), ServiceError> {
        // 幂等：未注册的项目直接视为已移除
        let Some(project) = self.project_repo.find_by_path(&cmd.path).await? else {
            return Ok(());
        };
        let pid = project.id;
        // 级联软删用统一窗口时间戳：恢复时按 deleted_at == 窗口值精确还原「随项目一起删的」对象，
        // 用户此前单独删除的会话/规则保持删除
        let window = fmt_time(&now_local());
        self.checkpoint_repo
            .soft_delete_by_project_window(pid, &window)
            .await?;
        self.message_repo
            .soft_delete_by_project_window(pid, &window)
            .await?;
        self.session_repo
            .soft_delete_by_project_window(pid, &window)
            .await?;
        self.perm_rule_repo
            .soft_delete_by_project_window(pid, &window)
            .await?;
        self.project_repo.soft_delete_with(pid, &window).await?;
        tracing::info!(project_id = pid, window = %window, "项目已移除（连带会话进回收站）");
        Ok(())
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
