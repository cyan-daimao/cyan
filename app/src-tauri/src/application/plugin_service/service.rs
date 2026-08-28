//! PluginService 实现：install（解包+注册内容物）/ enable（幂等重导入）/ disable（摘除效果）/ delete（卸载清理）。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::config::{
    McpRepository, McpServer, PermAction, PermRuleRepository, PermissionRule,
};
use crate::domain::plugin::{Plugin, PluginRepository};
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
    /// 插件市场搜索（GitHub topic:cyan-plugin）
    async fn search_marketplace(
        &self,
        query: SearchMarketplaceQuery,
    ) -> Result<Vec<MarketItemBO>, ServiceError>;
    /// 从 GitHub 仓库一键安装（下载 zip 后复用 zip 安装路径）
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
}

impl PluginServiceImpl {
    /// 构造（plugins_dir 注入，生产为 `~/.cyan/plugins`）
    pub fn new(
        plugin_repo: Arc<dyn PluginRepository>,
        mcp_repo: Arc<dyn McpRepository>,
        perm_repo: Arc<dyn PermRuleRepository>,
        plugins_dir: PathBuf,
    ) -> Self {
        Self {
            plugin_repo,
            mcp_repo,
            perm_repo,
            plugins_dir,
        }
    }

    /// 插件目录
    fn plugin_dir(&self, name: &str) -> PathBuf {
        self.plugins_dir.join(name)
    }

    /// 从包文件幂等注册内容物，返回 (技能数, MCP 数, 规则数)。
    /// MCP upsert 为 disabled 待用户启用；规则插入全局作用域；与用户自建内容同名冲突时报错。
    async fn register_contents(&self, name: &str, dir: &Path) -> Result<(i64, i64, i64), ServiceError> {
        // MCP 声明
        let mcp_decls = plugin_fs::read_mcp_decls(dir)?;
        for decl in &mcp_decls {
            match self.mcp_repo.find_by_name(&decl.name).await? {
                Some(mut s) if s.plugin_origin.as_deref() == Some(name) => {
                    // 本插件的记录（重导入）：恢复命令，保持 disabled 待用户启用
                    s.command = decl.command.clone();
                    self.mcp_repo.update(&s).await?;
                }
                Some(_) => {
                    return Err(ServiceError::conflict(format!(
                        "MCP 服务器名与已有配置冲突：{}",
                        decl.name
                    )));
                }
                None => {
                    let mut s = McpServer::new(decl.name.clone(), decl.command.clone(), now_local());
                    s.plugin_origin = Some(name.to_string());
                    self.mcp_repo.insert(&mut s).await?;
                }
            }
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
}

#[async_trait]
impl PluginService for PluginServiceImpl {
    async fn list_plugins(&self) -> Result<Vec<PluginBO>, ServiceError> {
        let plugins = self.plugin_repo.list().await?;
        Ok(plugins.into_iter().map(PluginBO::from).collect())
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
        if self.plugin_repo.find_by_name(&manifest.name).await?.is_some() {
            return Err(ServiceError::conflict(format!("插件已安装：{}", manifest.name)));
        }
        let dir = plugin_fs::extract_package(source, &self.plugins_dir, &manifest.name)?;
        let counts = match self.register_contents(&manifest.name, &dir).await {
            Ok(c) => c,
            Err(e) => {
                // 注册失败回滚：删目录，规则按 origin 清理
                let _ = self.perm_repo.soft_delete_by_plugin_origin(&manifest.name).await;
                let _ = std::fs::remove_dir_all(&dir);
                return Err(e);
            }
        };
        let mut plugin = Plugin::from_manifest(&manifest, counts, now_local());
        self.plugin_repo.insert(&mut plugin).await?;
        tracing::info!(plugin = %plugin.name, "插件安装完成");
        Ok(PluginBO::from(plugin))
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
        } else if plugin.disable() {
            self.revoke_contents(&plugin.name).await?;
        }
        self.plugin_repo.update(&plugin).await?;
        Ok(PluginBO::from(plugin))
    }

    async fn delete_plugin(&self, cmd: DeletePluginCmd) -> Result<(), ServiceError> {
        let Some(mut plugin) = self.plugin_repo.find_by_id(cmd.id).await? else {
            return Ok(()); // 幂等
        };
        if plugin.disable() {
            self.revoke_contents(&plugin.name).await?;
        }
        let dir = self.plugin_dir(&plugin.name);
        if dir.exists() {
            std::fs::remove_dir_all(&dir)
                .map_err(|e| ServiceError::external(format!("删除插件目录失败：{e}")))?;
        }
        self.plugin_repo.soft_delete(plugin.id).await?;
        tracing::info!(plugin = %plugin.name, "插件已卸载");
        Ok(())
    }

    async fn search_marketplace(
        &self,
        query: SearchMarketplaceQuery,
    ) -> Result<Vec<MarketItemBO>, ServiceError> {
        let items = plugin_fs::github::search_plugins(&query.keyword).await?;
        Ok(items.into_iter().map(MarketItemBO::from).collect())
    }

    async fn install_plugin_from_github(
        &self,
        cmd: InstallFromGithubCmd,
    ) -> Result<PluginBO, ServiceError> {
        // 校验 owner/repo 格式（1001），网络失败/限流走 anyhow → 3000
        plugin_fs::github::validate_full_name(&cmd.full_name)?;
        let tmp_zip = plugin_fs::github::download_repo_zip(&cmd.full_name).await?;
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

        // 卸载：目录删除 + 记录软删 + 内容物清理
        svc.delete_plugin(DeletePluginCmd { id: bo.id }).await.unwrap();
        assert!(!plugins_dir.path().join("test-plugin").exists());
        assert!(PluginRepositoryImpl::new(pool.clone())
            .find_by_id(bo.id)
            .await
            .unwrap()
            .is_none());
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
}
