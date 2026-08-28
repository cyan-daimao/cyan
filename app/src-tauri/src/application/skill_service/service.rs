//! SkillService 实现：三级合并（项目 > 插件 > 全局，同名高优先级覆盖）、按作用域落盘、
//! 技能市场（GitHub topic:cyan-skill 搜索 + 一键安装到全局目录）。

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;

use crate::application::plugin_service::MarketItemBO;
use crate::domain::plugin::{PluginRepository, PluginStatus};
use crate::domain::shared::ProjectPath;
use crate::domain::skill::{Skill, SkillSource};
use crate::error::ServiceError;
use crate::infra::fs::skill as skill_fs;
use crate::infra::plugin::github as github;
use crate::infra::plugin::unzip_to;

use super::{
    DeleteSkillCmd, InstallSkillFromGithubCmd, ListSkillQuery, SaveSkillCmd, SearchSkillMarketQuery,
    SkillBO,
};

/// 技能服务
#[async_trait]
pub trait SkillService: Send + Sync {
    /// 技能列表（全局 + 启用插件 + 项目合并，同名高优先级覆盖）
    async fn list_skills(&self, query: ListSkillQuery) -> Result<Vec<SkillBO>, ServiceError>;
    /// 保存技能（按 scope 写盘，文件名合法校验）
    async fn save_skill(&self, cmd: SaveSkillCmd) -> Result<SkillBO, ServiceError>;
    /// 删除技能（按 scope 删文件，幂等）
    async fn delete_skill(&self, cmd: DeleteSkillCmd) -> Result<(), ServiceError>;
    /// 技能市场搜索（GitHub topic:cyan-skill）
    async fn search_skill_market(
        &self,
        query: SearchSkillMarketQuery,
    ) -> Result<Vec<MarketItemBO>, ServiceError>;
    /// 从 GitHub 仓库一键安装技能到全局目录（frontmatter 注入 market 来源）
    async fn install_skill_from_github(
        &self,
        cmd: InstallSkillFromGithubCmd,
    ) -> Result<Vec<SkillBO>, ServiceError>;
}

/// 技能服务实现（文件存取 + 插件表查询启用状态；目录全部可注入，测试隔离）
pub struct SkillServiceImpl {
    plugin_repo: Arc<dyn PluginRepository>,
    /// 插件根目录（可注入，测试隔离）
    plugins_dir: PathBuf,
    /// 全局技能目录（可注入；生产为 `~/.cyan/skills`）
    global_skills_dir: PathBuf,
}

impl SkillServiceImpl {
    /// 构造
    pub fn new(
        plugin_repo: Arc<dyn PluginRepository>,
        plugins_dir: PathBuf,
        global_skills_dir: PathBuf,
    ) -> Self {
        Self {
            plugin_repo,
            plugins_dir,
            global_skills_dir,
        }
    }

    /// 解析可选项目根（空串视为无）
    fn resolve_root(project_path: Option<&str>) -> Result<Option<ProjectPath>, ServiceError> {
        match project_path {
            Some(p) if !p.trim().is_empty() => Ok(Some(ProjectPath::new(p)?)),
            _ => Ok(None),
        }
    }
}

#[async_trait]
impl SkillService for SkillServiceImpl {
    async fn list_skills(&self, query: ListSkillQuery) -> Result<Vec<SkillBO>, ServiceError> {
        // 合并顺序 = 优先级从低到高：全局 → 插件（按插件名排序保证确定性）→ 项目
        let mut merged: std::collections::BTreeMap<String, Skill> = skill_fs::scan_skills(
            &self.global_skills_dir,
            SkillSource::Global,
        )?
        .into_iter()
        .map(|s| (s.id.clone(), s))
        .collect();
        // 启用中插件的 skills/ 目录纳入扫描（禁用插件自动停扫）
        let mut plugins: Vec<_> = self
            .plugin_repo
            .list()
            .await?
            .into_iter()
            .filter(|p| p.status == PluginStatus::Enabled)
            .collect();
        plugins.sort_by(|a, b| a.name.cmp(&b.name));
        for plugin in plugins {
            let dir = self.plugins_dir.join(&plugin.name).join("skills");
            for s in skill_fs::scan_skills(&dir, SkillSource::Plugin(plugin.name.clone()))? {
                merged.insert(s.id.clone(), s);
            }
        }
        if let Some(root) = Self::resolve_root(Some(&query.project_path))? {
            let project_dir = skill_fs::project_skills_dir(&root)?;
            for s in skill_fs::scan_skills(&project_dir, SkillSource::Project)? {
                merged.insert(s.id.clone(), s);
            }
        }
        Ok(merged.into_values().map(SkillBO::from).collect())
    }

    async fn save_skill(&self, cmd: SaveSkillCmd) -> Result<SkillBO, ServiceError> {
        let source = SkillSource::parse(&cmd.scope)?;
        let is_project = source == SkillSource::Project;
        let skill = Skill {
            id: cmd.file_name,
            name: cmd.name,
            description: cmd.description,
            enabled: cmd.enabled,
            source,
            market_repo: None, // 手动创建无市场来源
            content: cmd.content,
        };
        skill.validate()?;
        let root = Self::resolve_root(cmd.project_path.as_deref())?;
        if is_project && root.is_none() {
            return Err(ServiceError::validation("项目级技能需要 projectPath"));
        }
        skill_fs::save_skill_file(skill.source.clone(), root.as_ref(), &skill)?;
        Ok(SkillBO::from(skill))
    }

    async fn delete_skill(&self, cmd: DeleteSkillCmd) -> Result<(), ServiceError> {
        let source = SkillSource::parse(&cmd.scope)?;
        let root = Self::resolve_root(cmd.project_path.as_deref())?;
        if source == SkillSource::Project && root.is_none() {
            return Err(ServiceError::validation("项目级技能需要 projectPath"));
        }
        skill_fs::delete_skill_file(source, root.as_ref(), &cmd.file_name)?;
        Ok(())
    }

    async fn search_skill_market(
        &self,
        query: SearchSkillMarketQuery,
    ) -> Result<Vec<MarketItemBO>, ServiceError> {
        let items = github::search_skills(&query.keyword).await?;
        Ok(items.into_iter().map(MarketItemBO::from).collect())
    }

    async fn install_skill_from_github(
        &self,
        cmd: InstallSkillFromGithubCmd,
    ) -> Result<Vec<SkillBO>, ServiceError> {
        // 校验 owner/repo 格式（1001），网络失败/限流走 anyhow → 3000
        github::validate_full_name(&cmd.full_name)?;
        let tmp_zip = github::download_repo_zip(&cmd.full_name).await?;
        // 解压到临时目录（剥离 GitHub zip 顶层包裹目录）
        let tmp_dir = tempfile::tempdir()
            .map_err(|e| ServiceError::external(format!("创建临时目录失败：{e}")))?;
        unzip_to(&tmp_zip, tmp_dir.path())?;
        // 收集技能文件并安装到全局目录（同名冲突/空仓库在 infra 层报错并回滚）
        let files = skill_fs::collect_skill_files(tmp_dir.path());
        let skills = skill_fs::install_skill_files(&self.global_skills_dir, &cmd.full_name, &files)?;
        tracing::info!(repo = %cmd.full_name, count = skills.len(), "技能安装完成");
        Ok(skills.into_iter().map(SkillBO::from).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::db::plugin_repo::PluginRepositoryImpl;
    use sqlx::SqlitePool;

    fn svc(pool: &SqlitePool, plugins_dir: PathBuf, global_dir: PathBuf) -> SkillServiceImpl {
        SkillServiceImpl::new(
            Arc::new(PluginRepositoryImpl::new(pool.clone())),
            plugins_dir,
            global_dir,
        )
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn project_overrides_global_on_same_id(pool: SqlitePool) {
        let tmp = tempfile::tempdir().unwrap();
        let root = ProjectPath::new(tmp.path()).unwrap();
        let global_dir = tempfile::tempdir().unwrap();
        let plugins_dir = tempfile::tempdir().unwrap();
        // 全局与项目各写同名技能
        let global_file = global_dir.path().join("dup-skill.md");
        std::fs::write(
            &global_file,
            skill_fs::serialize_skill("全局版", "g", true, None, "global body"),
        )
        .unwrap();
        let skill = Skill {
            id: "dup-skill".into(),
            name: "项目版".into(),
            description: "p".into(),
            enabled: true,
            source: SkillSource::Project,
            market_repo: None,
            content: "project body".into(),
        };
        skill_fs::save_skill_file(SkillSource::Project, Some(&root), &skill).unwrap();

        let list = svc(
            &pool,
            plugins_dir.path().to_path_buf(),
            global_dir.path().to_path_buf(),
        )
        .list_skills(ListSkillQuery {
            project_path: root.root().to_string_lossy().into_owned(),
        })
        .await
        .unwrap();
        let dup = list.iter().find(|s| s.id == "dup-skill").expect("应有同名技能");
        assert_eq!(dup.name, "项目版", "同名时项目级覆盖全局");
        assert_eq!(dup.source, "project");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn empty_project_path_lists_only_global(pool: SqlitePool) {
        // projectPath 为空串：不触项目目录，不应报错
        let plugins_dir = tempfile::tempdir().unwrap();
        let global_dir = tempfile::tempdir().unwrap();
        let list = svc(
            &pool,
            plugins_dir.path().to_path_buf(),
            global_dir.path().to_path_buf(),
        )
        .list_skills(ListSkillQuery {
            project_path: String::new(),
        })
        .await
        .unwrap();
        assert!(list.iter().all(|s| s.source == "global"));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn save_validates_and_writes(pool: SqlitePool) {
        let tmp = tempfile::tempdir().unwrap();
        let root_path = tmp.path().to_string_lossy().into_owned();
        let plugins_dir = tempfile::tempdir().unwrap();
        let global_dir = tempfile::tempdir().unwrap();
        let svc = svc(
            &pool,
            plugins_dir.path().to_path_buf(),
            global_dir.path().to_path_buf(),
        );
        // 非法文件名
        let bad = svc
            .save_skill(SaveSkillCmd {
                scope: "project".into(),
                file_name: "../evil".into(),
                name: "x".into(),
                description: String::new(),
                enabled: true,
                content: "c".into(),
                project_path: Some(root_path.clone()),
            })
            .await;
        assert!(bad.is_err());
        // project scope 缺 projectPath
        let no_root = svc
            .save_skill(SaveSkillCmd {
                scope: "project".into(),
                file_name: "ok-skill".into(),
                name: "x".into(),
                description: String::new(),
                enabled: true,
                content: "c".into(),
                project_path: None,
            })
            .await;
        assert!(no_root.is_err());
        // 正常保存（手动创建 market_repo = None）
        let bo = svc
            .save_skill(SaveSkillCmd {
                scope: "project".into(),
                file_name: "ok-skill".into(),
                name: "名称".into(),
                description: "d".into(),
                enabled: true,
                content: "c $ARGUMENTS".into(),
                project_path: Some(root_path),
            })
            .await
            .unwrap();
        assert_eq!(bo.id, "ok-skill");
        assert_eq!(bo.source, "project");
        assert_eq!(bo.market_repo, None);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn plugin_skills_scanned_with_priority(pool: SqlitePool) {
        use crate::domain::plugin::{Plugin, PluginManifest, PluginRepository};
        let plugins_dir = tempfile::tempdir().unwrap();
        // 造启用插件（含同名技能 + 独有技能）
        let pdir = plugins_dir.path().join("plug-a/skills");
        std::fs::create_dir_all(&pdir).unwrap();
        std::fs::write(
            pdir.join("dup-skill.md"),
            skill_fs::serialize_skill("插件版", "p", true, None, "plugin body"),
        )
        .unwrap();
        std::fs::write(
            pdir.join("plug-only.md"),
            skill_fs::serialize_skill("插件独有", "p", true, None, "body"),
        )
        .unwrap();
        let manifest = PluginManifest {
            name: "plug-a".into(),
            version: "1.0.0".into(),
            author: String::new(),
            description: String::new(),
            cyan_min_version: None,
            permissions: vec!["skills".into()],
        };
        let plugin_repo = PluginRepositoryImpl::new(pool.clone());
        let mut plugin = Plugin::from_manifest(&manifest, (2, 0, 0), crate::infra::db::now_local());
        plugin_repo.insert(&mut plugin).await.unwrap();

        // 项目同名技能
        let tmp = tempfile::tempdir().unwrap();
        let root = ProjectPath::new(tmp.path()).unwrap();
        let skill = Skill {
            id: "dup-skill".into(),
            name: "项目版".into(),
            description: "p".into(),
            enabled: true,
            source: SkillSource::Project,
            market_repo: None,
            content: "project body".into(),
        };
        skill_fs::save_skill_file(SkillSource::Project, Some(&root), &skill).unwrap();

        let global_dir = tempfile::tempdir().unwrap();
        let svc = svc(
            &pool,
            plugins_dir.path().to_path_buf(),
            global_dir.path().to_path_buf(),
        );
        let list = svc
            .list_skills(ListSkillQuery {
                project_path: root.root().to_string_lossy().into_owned(),
            })
            .await
            .unwrap();
        // 插件技能进入扫描
        let plug = list.iter().find(|s| s.id == "plug-only").expect("插件技能应入列");
        assert_eq!(plug.source, "plugin");
        assert_eq!(plug.plugin_name.as_deref(), Some("plug-a"));
        // 同名优先级：项目 > 插件
        let dup = list.iter().find(|s| s.id == "dup-skill").unwrap();
        assert_eq!(dup.name, "项目版");

        // 禁用插件后停扫
        let mut plugin = plugin_repo.find_by_id(plugin.id).await.unwrap().unwrap();
        plugin.disable();
        plugin_repo.update(&plugin).await.unwrap();
        let list = svc
            .list_skills(ListSkillQuery {
                project_path: String::new(),
            })
            .await
            .unwrap();
        assert!(list.iter().all(|s| s.id != "plug-only"), "禁用插件应停扫");
    }

    /// 本地构造 GitHub 风格 zip（顶层包裹目录 repo-sha/）后走安装逻辑（不打网络）
    #[sqlx::test(migrations = "./migrations")]
    async fn install_from_local_zip_end_to_end(pool: SqlitePool) {
        let src = tempfile::tempdir().unwrap();
        let zip_path = src.path().join("repo.zip");
        let file = std::fs::File::create(&zip_path).unwrap();
        let mut zw = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default();
        zw.start_file("skills-repo-abc/README.md", opts).unwrap();
        std::io::Write::write_all(&mut zw, b"# readme").unwrap();
        zw.start_file("skills-repo-abc/top-skill.md", opts).unwrap();
        std::io::Write::write_all(&mut zw, "---\nname: 顶层\n---\nbody $ARGUMENTS".as_bytes())
            .unwrap();
        zw.start_file("skills-repo-abc/skills/inner-skill.md", opts).unwrap();
        std::io::Write::write_all(&mut zw, "---\nname: 内层\n---\nbody2".as_bytes()).unwrap();
        zw.finish().unwrap();

        let global_dir = tempfile::tempdir().unwrap();
        // 解压 + 收集 + 安装（与 install_skill_from_github 的后半段同一条路径）
        let tmp_dir = tempfile::tempdir().unwrap();
        unzip_to(&zip_path, tmp_dir.path()).unwrap();
        // 顶层包裹已剥离
        assert!(tmp_dir.path().join("top-skill.md").exists());
        let files = skill_fs::collect_skill_files(tmp_dir.path());
        assert_eq!(files.len(), 2, "README 排除，顶层+skills/ 各 1 个");
        let skills =
            skill_fs::install_skill_files(global_dir.path(), "cy/skills-repo", &files).unwrap();
        assert_eq!(skills.len(), 2);
        let text =
            std::fs::read_to_string(global_dir.path().join("top-skill.md")).unwrap();
        assert!(text.contains("market: cy/skills-repo"));

        // 装入 SkillService 后能列出来且带 marketRepo
        let plugins_dir = tempfile::tempdir().unwrap();
        let svc = svc(
            &pool,
            plugins_dir.path().to_path_buf(),
            global_dir.path().to_path_buf(),
        );
        let list = svc
            .list_skills(ListSkillQuery {
                project_path: String::new(),
            })
            .await
            .unwrap();
        assert_eq!(list.len(), 2);
        assert!(list
            .iter()
            .all(|s| s.market_repo.as_deref() == Some("cy/skills-repo")));

        // 再次安装同仓库 → 同名冲突
        let err = skill_fs::install_skill_files(global_dir.path(), "cy/skills-repo", &files)
            .unwrap_err();
        assert!(matches!(err, crate::domain::DomainError::Conflict(_)));
    }
}
