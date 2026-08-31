//! PluginService 实现：install（解包+注册内容物）/ enable（幂等重导入）/ disable（摘除效果）/ delete（卸载清理）。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::config::{
    McpRepository, McpServer, McpTransport, PermAction, PermRuleRepository, PermissionRule,
};
use crate::domain::plugin::{Plugin, PluginRepository, SidecarGateway};
use crate::error::ServiceError;
use crate::infra::db::now_local;
use crate::infra::plugin as plugin_fs;

use super::{
    DeletePluginCmd, InstallFromGithubCmd, InstallPluginCmd, MarketItemBO, PluginBO,
    SearchMarketplaceQuery, TogglePluginCmd,
};

/// 插件服务
#[async_trait]
pub trait PluginService: Send + Sync {
    /// 插件列表
    async fn list_plugins(&self) -> Result<Vec<PluginBO>, ServiceError>;
    /// 安装插件（zip 或目录；同名冲突报错）
    async fn install_plugin(&self, cmd: InstallPluginCmd) -> Result<PluginBO, ServiceError>;
    /// 启停插件（enable=幂等重导入内容物；disable=摘除规则/禁用 MCP/技能停扫）
    async fn toggle_plugin(&self, cmd: TogglePluginCmd) -> Result<PluginBO, ServiceError>;
    /// 卸载插件（disable + 删目录 + 软删记录，幂等）
    async fn delete_plugin(&self, cmd: DeletePluginCmd) -> Result<(), ServiceError>;
    /// 插件市场搜索（GitHub topic:cyan-plugin / Gitee owner-repo 直达）
    async fn search_marketplace(
        &self,
        query: SearchMarketplaceQuery,
    ) -> Result<Vec<MarketItemBO>, ServiceError>;
    /// 从远端仓库一键安装（Gitee/GitHub；下载 zip 后复用 zip 安装路径）
    async fn install_plugin_from_github(
        &self,
        cmd: InstallFromGithubCmd,
    ) -> Result<PluginBO, ServiceError>;
}

/// 插件服务实现
pub struct PluginServiceImpl {
    plugin_repo: Arc<dyn PluginRepository>,
    mcp_repo: Arc<dyn McpRepository>,
    perm_repo: Arc<dyn PermRuleRepository>,
    /// 插件根目录（可注入，测试用 tempfile 隔离）
    plugins_dir: PathBuf,
    /// sidecar 管理端口（插件 v3：开关控制外部进程启停）
    sidecar: Arc<dyn SidecarGateway>,
}

impl PluginServiceImpl {
    /// 构造（plugins_dir 注入，生产为 `~/.cyan/plugins`）
    pub fn new(
        plugin_repo: Arc<dyn PluginRepository>,
        mcp_repo: Arc<dyn McpRepository>,
        perm_repo: Arc<dyn PermRuleRepository>,
        plugins_dir: PathBuf,
        sidecar: Arc<dyn SidecarGateway>,
    ) -> Self {
        Self {
            plugin_repo,
            mcp_repo,
            perm_repo,
            plugins_dir,
            sidecar,
        }
    }

    /// 插件目录
    fn plugin_dir(&self, name: &str) -> PathBuf {
        self.plugins_dir.join(name)
    }

    /// upsert 本插件的 MCP 记录（disabled 待用户启用；同 origin 更新 command；与用户自建同名冲突报错）
    async fn upsert_plugin_mcp(
        &self,
        origin: &str,
        name: &str,
        command: &str,
    ) -> Result<(), ServiceError> {
        match self.mcp_repo.find_by_name(name).await? {
            Some(mut s) if s.plugin_origin.as_deref() == Some(origin) => {
                s.transport = if McpTransport::is_remote_url(command) {
                    McpTransport::Sse
                } else {
                    McpTransport::Stdio
                };
                s.command = command.to_string();
                self.mcp_repo.update(&s).await?;
            }
            Some(_) => {
                return Err(ServiceError::conflict(format!(
                    "MCP 服务器名与已有配置冲突：{name}"
                )));
            }
            None => {
                // 重装自愈：软删行仍占 name UNIQUE，插入前清掉同名软删脏数据
                self.mcp_repo.hard_delete_by_name(name).await?;
                // sidecar URL / 声明命令：按前缀自动判定传输方式
                let transport = if McpTransport::is_remote_url(command) {
                    McpTransport::Sse
                } else {
                    McpTransport::Stdio
                };
                let mut s = McpServer::with_transport(name.to_string(), transport, command.to_string(), now_local());
                s.plugin_origin = Some(origin.to_string());
                self.mcp_repo.insert(&mut s).await?;
            }
        }
        Ok(())
    }

    /// 从包文件幂等注册内容物，返回 (技能数, MCP 数, 规则数)。
    /// MCP upsert 为 disabled 待用户启用；规则插入全局作用域；与用户自建内容同名冲突时报错。
    async fn register_contents(&self, name: &str, dir: &Path) -> Result<(i64, i64, i64), ServiceError> {
        // MCP 声明
        let mcp_decls = plugin_fs::read_mcp_decls(dir)?;
        for decl in &mcp_decls {
            self.upsert_plugin_mcp(name, &decl.name, &decl.command).await?;
        }
        // 权限规则声明（全局作用域，带 plugin_origin）
        let rule_decls = plugin_fs::read_rule_decls(dir)?;
        for decl in &rule_decls {
            let action = PermAction::parse(&decl.action)
                .ok_or_else(|| ServiceError::validation(format!("非法权限动作：{}", decl.action)))?;
            let mut rule = PermissionRule {
                id: 0,
                project_id: None,
                session_id: None,
                tool: decl.tool.clone(),
                pattern: decl.pattern.clone(),
                action,
                sort: decl.sort,
                plugin_origin: Some(name.to_string()),
                created_at: now_local(),
                updated_at: now_local(),
                deleted_at: None,
            };
            rule.validate()?;
            match self
                .perm_repo
                .find_by_tool_pattern(&decl.tool, &decl.pattern, None, None)
                .await?
            {
                Some(mut r) if r.plugin_origin.as_deref() == Some(name) => {
                    r.action = action;
                    r.sort = decl.sort;
                    self.perm_repo.update(&r).await?;
                }
                Some(_) => {
                    return Err(ServiceError::conflict(format!(
                        "权限规则与已有全局规则冲突：{} {}",
                        decl.tool, decl.pattern
                    )));
                }
                None => self.perm_repo.insert(&mut rule).await?,
            }
        }
        Ok((
            plugin_fs::count_skills(dir),
            mcp_decls.len() as i64,
            rule_decls.len() as i64,
        ))
    }

    /// 摘除插件内容物效果：规则软删 + 本插件 MCP 记录禁用（技能因只扫启用插件而自动停扫）
    async fn revoke_contents(&self, name: &str) -> Result<(), ServiceError> {
        self.perm_repo.soft_delete_by_plugin_origin(name).await?;
        for mut s in self
            .mcp_repo
            .list()
            .await?
            .into_iter()
            .filter(|s| s.plugin_origin.as_deref() == Some(name))
        {
            s.disable();
            self.mcp_repo.update(&s).await?;
        }
        Ok(())
    }

    /// 启动插件声明的 sidecar 后端（未声明返回 (None, 0)）：
    /// 分配端口 → spawn → 健康检查 → 声明了 mcp 则 upsert MCP 记录；返回 (分配端口, 新增 MCP 数)
    async fn start_backend_if_declared(
        &self,
        name: &str,
        dir: &Path,
    ) -> Result<(Option<u16>, i64), ServiceError> {
        let manifest = plugin_fs::read_installed_manifest(dir)?;
        let Some(backend) = &manifest.backend else {
            return Ok((None, 0));
        };
        let info = self
            .sidecar
            .start(name, dir, &backend.command, backend.health_path.as_deref())
            .await
            .map_err(|e| ServiceError::external(format!("sidecar 启动失败，已回滚：{e:#}")))?;
        let mut mcp_added = 0;
        if let Some(mcp) = &backend.mcp {
            let url = mcp.url.replace("{port}", &info.port.to_string());
            self.upsert_plugin_mcp(name, &mcp.name, &url).await?;
            mcp_added = 1;
        }
        Ok((Some(info.port), mcp_added))
    }

    /// 用实时 sidecar 状态填充 BO（含 frontendUrl 的 {port} 替换，仅运行中时有值）
    fn fill_backend_status(&self, bo: &mut PluginBO) {
        let status = self.sidecar.status(&bo.name);
        bo.backend_running = status.running;
        bo.backend_port = status.port;
        if let Some(port) = status.port.filter(|_| status.running) {
            bo.backend_frontend_url =
                plugin_fs::read_installed_manifest(&self.plugin_dir(&bo.name))
                    .ok()
                    .and_then(|m| m.backend.and_then(|b| b.frontend_url))
                    .map(|url| url.replace("{port}", &port.to_string()));
        }
    }
}

#[async_trait]
impl PluginService for PluginServiceImpl {
    async fn list_plugins(&self) -> Result<Vec<PluginBO>, ServiceError> {
        let plugins = self.plugin_repo.list().await?;
        Ok(plugins
            .into_iter()
            .map(|p| {
                let mut bo = PluginBO::from(p);
                self.fill_backend_status(&mut bo);
                bo
            })
            .collect())
    }

    async fn install_plugin(&self, cmd: InstallPluginCmd) -> Result<PluginBO, ServiceError> {
        let source = Path::new(cmd.source_path.trim());
        if !source.exists() {
            return Err(ServiceError::not_found(format!(
                "插件源不存在：{}",
                source.display()
            )));
        }
        let manifest = plugin_fs::read_manifest_from_source(source)?;
        let mut plugin = Plugin::from_manifest(&manifest, (0, 0, 0), now_local());
        // 清掉同名软删脏数据：旧版本卸载走软删，行仍占用 name UNIQUE 约束，
        // 重装 INSERT 会报 (2067) UNIQUE constraint failed: cyan_plugin.name
        self.plugin_repo.hard_delete_by_name(&manifest.name).await?;
        if self.plugin_repo.find_by_name(&manifest.name).await?.is_some() {
            return Err(ServiceError::conflict(format!("插件已安装：{}", manifest.name)));
        }
        // 无记录的同名残留目录：上次安装中断/落库失败的孤儿目录，删除后重装（自愈）
        let dir = self.plugin_dir(&manifest.name);
        if dir.exists() {
            tracing::warn!(
                plugin = %manifest.name,
                dir = %dir.display(),
                "发现无插件记录的同名残留目录，删除后重装"
            );
            std::fs::remove_dir_all(&dir)
                .map_err(|e| ServiceError::external(format!("残留插件目录删除失败：{e}")))?;
        }
        let dir = plugin_fs::extract_package(source, &self.plugins_dir, &manifest.name)?;
        let counts = match self.register_contents(&manifest.name, &dir).await {
            Ok(c) => c,
            Err(e) => {
                // 注册失败回滚：停 sidecar、删目录，规则按 origin 清理
                self.sidecar.stop(&manifest.name).await;
                let _ = self.perm_repo.soft_delete_by_plugin_origin(&manifest.name).await;
                let _ = std::fs::remove_dir_all(&dir);
                return Err(e);
            }
        };
        plugin.skill_count = counts.0;
        plugin.mcp_count = counts.1;
        plugin.rule_count = counts.2;
        // 声明了 sidecar 后端的插件安装即启用 → 安装时同步启动 sidecar（失败整体回滚）
        if manifest.backend.is_some() {
            if let Err(e) = self.start_backend_if_declared(&manifest.name, &dir).await {
                self.sidecar.stop(&manifest.name).await;
                let _ = self.perm_repo.soft_delete_by_plugin_origin(&manifest.name).await;
                let _ = std::fs::remove_dir_all(&dir);
                return Err(e);
            }
        }
        // sidecar mcp 注册计入 mcp_count（install 时 backend.mcp 已 upsert）
        if let Some(backend) = &manifest.backend {
            if backend.mcp.is_some() {
                plugin.mcp_count += 1;
            }
        }
        if let Err(e) = self.plugin_repo.insert(&mut plugin).await {
            // 落库失败回滚：停 sidecar、摘内容物、删目录；否则残留孤儿目录，重装报"插件目录已存在"
            self.sidecar.stop(&manifest.name).await;
            let _ = self.perm_repo.soft_delete_by_plugin_origin(&manifest.name).await;
            let _ = std::fs::remove_dir_all(&dir);
            return Err(e.into());
        }
        tracing::info!(plugin = %plugin.name, "插件安装完成");
        let mut bo = PluginBO::from(plugin);
        self.fill_backend_status(&mut bo);
        Ok(bo)
    }

    async fn toggle_plugin(&self, cmd: TogglePluginCmd) -> Result<PluginBO, ServiceError> {
        let mut plugin = self
            .plugin_repo
            .find_by_id(cmd.id)
            .await?
            .ok_or_else(|| ServiceError::not_found(format!("插件不存在：{}", cmd.id)))?;
        if cmd.enable {
            if !plugin.enable() {
                return Ok(PluginBO::from(plugin)); // 幂等
            }
            let dir = self.plugin_dir(&plugin.name);
            if !dir.is_dir() {
                return Err(ServiceError::not_found(format!(
                    "插件目录缺失，无法启用：{}",
                    dir.display()
                )));
            }
            let counts = self.register_contents(&plugin.name, &dir).await?;
            plugin.skill_count = counts.0;
            plugin.mcp_count = counts.1;
            plugin.rule_count = counts.2;
            // sidecar 后端：分配端口 → spawn → 健康检查 → MCP 记录；失败整体回滚（保持 disabled）
            if let Err(e) = self.start_backend_if_declared(&plugin.name, &dir).await {
                self.sidecar.stop(&plugin.name).await;
                self.revoke_contents(&plugin.name).await?;
                plugin.disable();
                self.plugin_repo.update(&plugin).await?;
                return Err(e);
            }
            // sidecar mcp 计入 mcp_count
            let manifest = plugin_fs::read_installed_manifest(&dir)?;
            if manifest.backend.as_ref().and_then(|b| b.mcp.as_ref()).is_some() {
                plugin.mcp_count += 1;
            }
        } else if plugin.disable() {
            // 先停 sidecar 再摘除内容物
            self.sidecar.stop(&plugin.name).await;
            self.revoke_contents(&plugin.name).await?;
        }
        self.plugin_repo.update(&plugin).await?;
        let mut bo = PluginBO::from(plugin);
        self.fill_backend_status(&mut bo);
        Ok(bo)
    }

    async fn delete_plugin(&self, cmd: DeletePluginCmd) -> Result<(), ServiceError> {
        let Some(mut plugin) = self.plugin_repo.find_by_id(cmd.id).await? else {
            return Ok(()); // 幂等
        };
        // 卸载 = disable（含 sidecar 停掉）+ 删目录 + 物理删除记录（幂等）。
        // 不走软删：软删行占用 name UNIQUE 约束，重装 INSERT 会报 UNIQUE 冲突。
        self.sidecar.stop(&plugin.name).await;
        if plugin.disable() {
            self.revoke_contents(&plugin.name).await?;
        }
        let dir = self.plugin_dir(&plugin.name);
        if dir.exists() {
            std::fs::remove_dir_all(&dir)
                .map_err(|e| ServiceError::external(format!("删除插件目录失败：{e}")))?;
        }
        self.plugin_repo.hard_delete(plugin.id).await?;
        tracing::info!(plugin = %plugin.name, "插件已卸载");
        Ok(())
    }

    async fn search_marketplace(
        &self,
        query: SearchMarketplaceQuery,
    ) -> Result<Vec<MarketItemBO>, ServiceError> {
        // Gitee 搜索接口匿名调用常被风控拦成空数组：关键字形如 owner/repo 时走直达详情，
        // 否则明确提示（Gitee 不支持关键字搜索），不让用户干等一个必然为空的结果
        if query.source.eq_ignore_ascii_case("gitee") {
            let kw = query.keyword.trim();
            if kw.is_empty() {
                return Err(ServiceError::validation(
                    "Gitee 源暂不支持浏览推荐列表，请输入 owner/repo 直接安装，或切换回 GitHub 源",
                ));
            }
            if crate::infra::plugin::github::validate_full_name(kw).is_err() {
                return Err(ServiceError::validation(
                    "Gitee 源请输入 owner/repo 形式的仓库地址（如 openharmony-sig/xxx）",
                ));
            }
            let meta = plugin_fs::gitee::repo_detail(kw)
                .await
                .map_err(|e| ServiceError::external(format!("{e:#}")))?;
            return Ok(vec![MarketItemBO::from(meta.item)]);
        }
        let items = plugin_fs::github::search_plugins(&query.keyword).await?;
        Ok(items.into_iter().map(MarketItemBO::from).collect())
    }

    async fn install_plugin_from_github(
        &self,
        cmd: InstallFromGithubCmd,
    ) -> Result<PluginBO, ServiceError> {
        // 校验 owner/repo 格式（1001）；Gitee 网络失败/限流走 anyhow → 3000
        plugin_fs::github::validate_full_name(&cmd.full_name)?;
        let tmp_zip = if cmd.is_gitee() {
            plugin_fs::gitee::download_repo_zip(&cmd.full_name)
                .await
                .map_err(|e| ServiceError::external(format!("{e:#}")))?
        } else {
            plugin_fs::github::download_repo_zip(&cmd.full_name).await?
        };
        // 复用 zip 安装路径；TempPath 绑定存活到安装结束
        self.install_plugin(InstallPluginCmd {
            source_path: tmp_zip.to_string_lossy().into_owned(),
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::config::{McpStatus, RuleScope};
    use crate::infra::db::mcp_repo::McpRepositoryImpl;
    use crate::infra::db::perm_rule_repo::PermRuleRepositoryImpl;
    use crate::infra::db::plugin_repo::PluginRepositoryImpl;
    use sqlx::SqlitePool;

    fn svc(pool: &SqlitePool, plugins_dir: PathBuf) -> PluginServiceImpl {
        PluginServiceImpl::new(
            Arc::new(PluginRepositoryImpl::new(pool.clone())),
            Arc::new(McpRepositoryImpl::new(pool.clone())),
            Arc::new(PermRuleRepositoryImpl::new(pool.clone())),
            plugins_dir,
            Arc::new(crate::infra::sidecar::SidecarManager::new()),
        )
    }

    /// 造一个目录形式的插件包
    fn make_pkg(src: &Path) {
        std::fs::create_dir_all(src.join("skills")).unwrap();
        std::fs::write(
            src.join("manifest.json"),
            r#"{"name":"test-plugin","version":"1.0.0","author":"a","description":"d","permissions":["skills","mcp","rules"]}"#,
        )
        .unwrap();
        std::fs::write(src.join("skills/s1.md"), "---\nname: S1\n---\nbody $ARGUMENTS").unwrap();
        std::fs::write(src.join("mcp.json"), r#"[{"name":"tp-fs","command":"npx mcp-fs"}]"#).unwrap();
        std::fs::write(
            src.join("rules.json"),
            r#"[{"tool":"Bash","pattern":"tp *","action":"allow","sort":5}]"#,
        )
        .unwrap();
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn install_toggle_delete_roundtrip(pool: SqlitePool) {
        let src = tempfile::tempdir().unwrap();
        make_pkg(src.path());
        let plugins_dir = tempfile::tempdir().unwrap();
        let svc = svc(&pool, plugins_dir.path().to_path_buf());

        // 安装
        let bo = svc
            .install_plugin(InstallPluginCmd {
                source_path: src.path().to_string_lossy().into_owned(),
            })
            .await
            .unwrap();
        assert_eq!(bo.status, "enabled");
        assert_eq!((bo.skill_count, bo.mcp_count, bo.rule_count), (1, 1, 1));
        assert!(plugins_dir.path().join("test-plugin/manifest.json").exists());
        // 内容物已注册
        let mcp_repo = McpRepositoryImpl::new(pool.clone());
        let mcp = mcp_repo.find_by_name("tp-fs").await.unwrap().expect("MCP 已注册");
        assert_eq!(mcp.plugin_origin.as_deref(), Some("test-plugin"));
        assert_eq!(mcp.status, McpStatus::Disabled, "插件 MCP 注册后待用户启用");
        let perm_repo = PermRuleRepositoryImpl::new(pool.clone());
        let rule = perm_repo
            .find_by_tool_pattern("Bash", "tp *", None, None)
            .await
            .unwrap()
            .expect("规则已注册");
        assert_eq!(rule.scope(), RuleScope::Global);

        // 同名冲突
        let dup = svc
            .install_plugin(InstallPluginCmd {
                source_path: src.path().to_string_lossy().into_owned(),
            })
            .await;
        assert!(dup.is_err());

        // 禁用：规则摘除 + MCP 保持 disabled
        let bo = svc
            .toggle_plugin(TogglePluginCmd { id: bo.id, enable: false })
            .await
            .unwrap();
        assert_eq!(bo.status, "disabled");
        assert!(perm_repo
            .find_by_tool_pattern("Bash", "tp *", None, None)
            .await
            .unwrap()
            .is_none());

        // 启用：幂等重导入（规则回来、MCP 记录恢复且仍 disabled）
        let bo = svc
            .toggle_plugin(TogglePluginCmd { id: bo.id, enable: true })
            .await
            .unwrap();
        assert_eq!(bo.status, "enabled");
        assert!(perm_repo
            .find_by_tool_pattern("Bash", "tp *", None, None)
            .await
            .unwrap()
            .is_some());
        let mcp = mcp_repo.find_by_name("tp-fs").await.unwrap().unwrap();
        assert_eq!(mcp.status, McpStatus::Disabled);
        // 幂等：重复 enable 无副作用
        let again = svc
            .toggle_plugin(TogglePluginCmd { id: bo.id, enable: true })
            .await
            .unwrap();
        assert_eq!(again.status, "enabled");

        // 卸载：目录删除 + 记录物理删除（不进回收站）+ 内容物清理
        svc.delete_plugin(DeletePluginCmd { id: bo.id }).await.unwrap();
        assert!(!plugins_dir.path().join("test-plugin").exists());
        assert!(PluginRepositoryImpl::new(pool.clone())
            .find_by_id(bo.id)
            .await
            .unwrap()
            .is_none());
        // 物理删除：软删列表（回收站）也查无此记录
        assert!(PluginRepositoryImpl::new(pool.clone())
            .list_deleted()
            .await
            .unwrap()
            .is_empty());
        assert!(perm_repo
            .find_by_tool_pattern("Bash", "tp *", None, None)
            .await
            .unwrap()
            .is_none());
        assert_eq!(
            mcp_repo.find_by_name("tp-fs").await.unwrap().unwrap().status,
            McpStatus::Disabled
        );
        // 幂等卸载
        svc.delete_plugin(DeletePluginCmd { id: bo.id }).await.unwrap();
    }

    /// insert 必失败的仓储装饰器（验证落库失败回滚，防孤儿目录）
    struct FailInsertRepo(Arc<dyn PluginRepository>);

    #[async_trait]
    impl PluginRepository for FailInsertRepo {
        async fn list(&self) -> anyhow::Result<Vec<Plugin>> {
            self.0.list().await
        }
        async fn find_by_id(&self, id: i64) -> anyhow::Result<Option<Plugin>> {
            self.0.find_by_id(id).await
        }
        async fn find_by_name(&self, name: &str) -> anyhow::Result<Option<Plugin>> {
            self.0.find_by_name(name).await
        }
        async fn insert(&self, _plugin: &mut Plugin) -> anyhow::Result<()> {
            Err(anyhow::anyhow!("模拟落库失败（如磁盘满/约束冲突）"))
        }
        async fn update(&self, plugin: &Plugin) -> anyhow::Result<()> {
            self.0.update(plugin).await
        }
        async fn soft_delete(&self, id: i64) -> anyhow::Result<()> {
            self.0.soft_delete(id).await
        }
        async fn hard_delete(&self, id: i64) -> anyhow::Result<()> {
            self.0.hard_delete(id).await
        }
        async fn hard_delete_by_name(&self, name: &str) -> anyhow::Result<()> {
            self.0.hard_delete_by_name(name).await
        }
        async fn list_deleted(&self) -> anyhow::Result<Vec<Plugin>> {
            self.0.list_deleted().await
        }
        async fn restore(&self, id: i64) -> anyhow::Result<()> {
            self.0.restore(id).await
        }
    }

    /// 无记录的同名残留目录：重装自愈（先删孤儿目录再装），不再报"插件目录已存在"
    #[sqlx::test(migrations = "./migrations")]
    async fn install_self_heals_orphan_dir(pool: SqlitePool) {
        let src = tempfile::tempdir().unwrap();
        make_pkg(src.path());
        let plugins_dir = tempfile::tempdir().unwrap();
        // 模拟上次安装中断留下的孤儿目录（无插件记录，内容是残留物）
        let orphan = plugins_dir.path().join("test-plugin");
        std::fs::create_dir_all(orphan.join("stale")).unwrap();
        std::fs::write(orphan.join("stale/leftover.txt"), "x").unwrap();

        let svc = svc(&pool, plugins_dir.path().to_path_buf());
        let bo = svc
            .install_plugin(InstallPluginCmd {
                source_path: src.path().to_string_lossy().into_owned(),
            })
            .await
            .unwrap();
        assert_eq!(bo.name, "test-plugin");
        // 残留物被清掉，目录为全新安装内容
        assert!(!orphan.join("stale").exists());
        assert!(orphan.join("manifest.json").exists());
    }

    /// 落库失败整体回滚：停 sidecar、摘规则、删目录，不留孤儿目录
    #[sqlx::test(migrations = "./migrations")]
    async fn install_insert_failure_rolls_back_dir(pool: SqlitePool) {
        let src = tempfile::tempdir().unwrap();
        make_pkg(src.path());
        let plugins_dir = tempfile::tempdir().unwrap();
        let svc_fail = PluginServiceImpl::new(
            Arc::new(FailInsertRepo(Arc::new(PluginRepositoryImpl::new(pool.clone())))),
            Arc::new(McpRepositoryImpl::new(pool.clone())),
            Arc::new(PermRuleRepositoryImpl::new(pool.clone())),
            plugins_dir.path().to_path_buf(),
            Arc::new(crate::infra::sidecar::SidecarManager::new()),
        );

        let err = svc_fail
            .install_plugin(InstallPluginCmd {
                source_path: src.path().to_string_lossy().into_owned(),
            })
            .await
            .unwrap_err();
        assert!(err.message.contains("模拟落库失败"));
        // 回滚后无孤儿目录、规则被摘除，重装可成功
        assert!(
            !plugins_dir.path().join("test-plugin").exists(),
            "落库失败后不应残留插件目录"
        );
        let perm_repo = PermRuleRepositoryImpl::new(pool.clone());
        assert!(perm_repo
            .find_by_tool_pattern("Bash", "tp *", None, None)
            .await
            .unwrap()
            .is_none());

        // 干净状态下重装成功
        let svc_ok = svc(&pool, plugins_dir.path().to_path_buf());
        assert!(svc_ok
            .install_plugin(InstallPluginCmd {
                source_path: src.path().to_string_lossy().into_owned(),
            })
            .await
            .is_ok());
    }

    /// 旧版卸载（软删）遗留的脏数据：行占用 name UNIQUE，重装 INSERT 报 (2067)。
    /// 安装前按名清理软删行 → 重装成功。
    #[sqlx::test(migrations = "./migrations")]
    async fn install_purges_soft_deleted_same_name_row(pool: SqlitePool) {
        let src = tempfile::tempdir().unwrap();
        make_pkg(src.path());
        let plugins_dir = tempfile::tempdir().unwrap();
        let svc = svc(&pool, plugins_dir.path().to_path_buf());

        // 安装 → 模拟旧版卸载（直接软删，目录已删）
        let bo = svc
            .install_plugin(InstallPluginCmd {
                source_path: src.path().to_string_lossy().into_owned(),
            })
            .await
            .unwrap();
        PluginRepositoryImpl::new(pool.clone())
            .soft_delete(bo.id)
            .await
            .unwrap();
        assert_eq!(
            PluginRepositoryImpl::new(pool.clone()).list_deleted().await.unwrap().len(),
            1,
            "前置：软删行已存在（旧版卸载遗留）"
        );

        // 重装：软删脏行被自动清理，INSERT 不再撞 UNIQUE
        let reinstalled = svc
            .install_plugin(InstallPluginCmd {
                source_path: src.path().to_string_lossy().into_owned(),
            })
            .await
            .unwrap();
        assert_eq!(reinstalled.name, "test-plugin");
        assert!(PluginRepositoryImpl::new(pool.clone())
            .list_deleted()
            .await
            .unwrap()
            .is_empty(), "软删脏行应被清理");
    }

    /// 插件 MCP 重装自愈：软删的同名 MCP 记录占 name UNIQUE（旧版卸载遗留），
    /// 安装时按名清理后再注册，不再报 (2067) UNIQUE constraint failed: cyan_mcp_server.name
    #[sqlx::test(migrations = "./migrations")]
    async fn install_heals_soft_deleted_plugin_mcp_row(pool: SqlitePool) {
        let src = tempfile::tempdir().unwrap();
        make_pkg(src.path());
        let plugins_dir = tempfile::tempdir().unwrap();
        let svc = svc(&pool, plugins_dir.path().to_path_buf());

        // 预置一条同名软删 MCP 脏数据（模拟旧版卸载遗留），无任何在用记录
        let now = crate::infra::db::now_local();
        sqlx::query(
            "INSERT INTO cyan_mcp_server (name, command, plugin_origin, created_by, updated_by, created_at, updated_at, deleted_at)
             VALUES ('tp-fs', 'npx old', 'test-plugin', 'local', 'local', ?, ?, ?)",
        )
        .bind(crate::infra::db::fmt_time(&now))
        .bind(crate::infra::db::fmt_time(&now))
        .bind(crate::infra::db::fmt_time(&now))
        .execute(&pool)
        .await
        .unwrap();

        // 安装成功：软删脏行被清理，新记录正常落库
        svc.install_plugin(InstallPluginCmd {
            source_path: src.path().to_string_lossy().into_owned(),
        })
        .await
        .unwrap();
        let mcp_repo = McpRepositoryImpl::new(pool.clone());
        let mcp = mcp_repo
            .find_by_name("tp-fs")
            .await
            .unwrap()
            .expect("MCP 应重新注册");
        assert_eq!(mcp.plugin_origin.as_deref(), Some("test-plugin"));
        assert!(mcp_repo.list_deleted().await.unwrap().is_empty(), "软删脏行应被清理");
    }

    /// 卸载后立即重装：记录已物理删除，UNIQUE 约束不再阻挡
    #[sqlx::test(migrations = "./migrations")]
    async fn delete_then_reinstall_roundtrip(pool: SqlitePool) {
        let src = tempfile::tempdir().unwrap();
        make_pkg(src.path());
        let plugins_dir = tempfile::tempdir().unwrap();
        let svc = svc(&pool, plugins_dir.path().to_path_buf());

        let bo = svc
            .install_plugin(InstallPluginCmd {
                source_path: src.path().to_string_lossy().into_owned(),
            })
            .await
            .unwrap();
        svc.delete_plugin(DeletePluginCmd { id: bo.id }).await.unwrap();

        // 重装成功（修复前：软删行占名 → UNIQUE constraint failed）
        let again = svc
            .install_plugin(InstallPluginCmd {
                source_path: src.path().to_string_lossy().into_owned(),
            })
            .await
            .unwrap();
        assert_eq!(again.name, "test-plugin");
    }

    /// 造一个带 sidecar backend 的插件包（command 可配）
    fn make_backend_pkg(src: &Path, command: &str, health_path: Option<&str>) {
        let health = health_path
            .map(|h| format!(r#","healthPath":"{h}""#))
            .unwrap_or_default();
        std::fs::create_dir_all(src).unwrap();
        std::fs::write(
            src.join("manifest.json"),
            format!(
                r#"{{"name":"backend-plugin","version":"1.0.0","author":"","description":"d","permissions":["backend","rules"],"backend":{{"command":"{command}"{health},"mcp":{{"name":"bp-mcp","url":"http://127.0.0.1:{{port}}/sse"}}}}}}"#
            ),
        )
        .unwrap();
        std::fs::write(
            src.join("rules.json"),
            r#"[{"tool":"Bash","pattern":"bp *","action":"allow"}]"#,
        )
        .unwrap();
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn install_sidecar_failure_rolls_back(pool: SqlitePool) {
        let src = tempfile::tempdir().unwrap();
        // `false` 立即退出 → 健康检查快速失败
        make_backend_pkg(src.path(), "false", Some("/health"));
        let plugins_dir = tempfile::tempdir().unwrap();
        let svc = svc(&pool, plugins_dir.path().to_path_buf());

        // 安装即启动 sidecar：失败整体回滚（目录删除、规则摘除、不落插件行）
        let err = svc
            .install_plugin(InstallPluginCmd {
                source_path: src.path().to_string_lossy().into_owned(),
            })
            .await
            .unwrap_err();
        assert!(err.message.contains("sidecar 启动失败"));
        assert!(!plugins_dir.path().join("backend-plugin").exists(), "回滚后插件目录应删除");
        assert!(PluginRepositoryImpl::new(pool.clone())
            .find_by_name("backend-plugin")
            .await
            .unwrap()
            .is_none(), "回滚后不应落插件记录");
        let perm_repo = PermRuleRepositoryImpl::new(pool.clone());
        assert!(perm_repo
            .find_by_tool_pattern("Bash", "bp *", None, None)
            .await
            .unwrap()
            .is_none(), "回滚后规则应被摘除");
        let mcp_repo = McpRepositoryImpl::new(pool.clone());
        assert!(mcp_repo.find_by_name("bp-mcp").await.unwrap().is_none());
        let list = svc.list_plugins().await.unwrap();
        assert!(list.is_empty());
    }

    #[sqlx::test(migrations = "./migrations")]
    #[allow(clippy::await_holding_lock)] // 测试锁故意跨 await 持有：串行化端口分配
    async fn sidecar_full_lifecycle(pool: SqlitePool) {
        // python3 可用性检查（无则跳过）
        if std::process::Command::new("python3")
            .arg("--version")
            .output()
            .is_err()
        {
            return;
        }
        // 真实起 HTTP 服务的测试串行化（防与其他 sidecar 测试的端口竞争）
        let _guard = crate::infra::sidecar::tests::http_test_lock();
        let src = tempfile::tempdir().unwrap();
        make_backend_pkg(
            src.path(),
            "python3 -m http.server {port} --bind 127.0.0.1",
            Some("/"),
        );
        let plugins_dir = tempfile::tempdir().unwrap();
        let svc = svc(&pool, plugins_dir.path().to_path_buf());

        // 安装即启用 sidecar：就绪 + MCP 记录注册（URL 端口已替换）
        let bo = svc
            .install_plugin(InstallPluginCmd {
                source_path: src.path().to_string_lossy().into_owned(),
            })
            .await
            .unwrap();
        assert!(bo.backend_running, "安装后 sidecar 应运行");
        let port = bo.backend_port.expect("应有分配端口");
        assert!((18700..=18799).contains(&port));
        assert_eq!(bo.mcp_count, 1);
        let mcp_repo = McpRepositoryImpl::new(pool.clone());
        let mcp = mcp_repo.find_by_name("bp-mcp").await.unwrap().expect("MCP 已注册");
        assert_eq!(mcp.plugin_origin.as_deref(), Some("backend-plugin"));
        assert_eq!(mcp.command, format!("http://127.0.0.1:{port}/sse"));
        assert_eq!(mcp.status, McpStatus::Disabled, "注册后仍待用户启用");
        let resp = reqwest::get(format!("http://127.0.0.1:{port}/")).await.unwrap();
        assert!(resp.status().is_success());

        // disable：sidecar 停掉 + 内容物摘除
        let bo = svc
            .toggle_plugin(TogglePluginCmd { id: bo.id, enable: false })
            .await
            .unwrap();
        assert!(!bo.backend_running);
        assert!(reqwest::get(format!("http://127.0.0.1:{port}/")).await.is_err());
        assert!(mcp_repo
            .find_by_name("bp-mcp")
            .await
            .unwrap()
            .map(|m| m.status == McpStatus::Disabled)
            .unwrap_or(false));

        // 再 enable：幂等重导入 + sidecar 重启 + MCP 恢复
        let bo = svc
            .toggle_plugin(TogglePluginCmd { id: bo.id, enable: true })
            .await
            .unwrap();
        assert!(bo.backend_running);
        assert!(bo.backend_port.is_some());
        assert!(mcp_repo.find_by_name("bp-mcp").await.unwrap().is_some());

        // delete：幂等清理（含 sidecar 停掉）
        svc.delete_plugin(DeletePluginCmd { id: bo.id }).await.unwrap();
        assert!(!svc.list_plugins().await.unwrap().first().map(|p| p.backend_running).unwrap_or(false));
    }
}
