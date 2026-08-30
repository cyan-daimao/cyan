//! ConfigService 实现：模型 / MCP / 权限规则 CRUD 编排。

use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::config::{
    McpRepository, McpServer, McpTransport, ModelConfig, ModelRepository, ModelStatus, PermAction,
    PermRuleRepository, PermissionRule, RuleScope,
};
use crate::error::ServiceError;
use crate::infra::db::now_local;
use crate::infra::mcp::McpGateway;
use crate::infra::{mcp as mcp_infra, mcp_registry, secret};

use super::{
    DeleteMcpCmd, DeleteModelCmd, DeletePermRuleCmd, McpMarketItemBO, McpServerBO, ModelBO,
    PermRuleBO, SaveMcpCmd, SaveModelCmd, SavePermRuleCmd, SearchMcpMarketQuery, SetDefaultModelCmd,
    ToggleMcpCmd,
};

/// 配置服务
#[async_trait]
pub trait ConfigService: Send + Sync {
    /// 模型列表
    async fn list_models(&self) -> Result<Vec<ModelBO>, ServiceError>;
    /// 保存模型（按 name upsert；API Key 写 keychain）
    async fn save_model(&self, cmd: SaveModelCmd) -> Result<ModelBO, ServiceError>;
    /// 删除模型（默认保护）
    async fn delete_model(&self, cmd: DeleteModelCmd) -> Result<(), ServiceError>;
    /// 设为默认（应用层保证唯一）
    async fn set_default_model(&self, cmd: SetDefaultModelCmd) -> Result<(), ServiceError>;
    /// MCP 服务器列表
    async fn list_mcp_servers(&self) -> Result<Vec<McpServerBO>, ServiceError>;
    /// 保存 MCP 服务器（按 name upsert）
    async fn save_mcp_server(&self, cmd: SaveMcpCmd) -> Result<McpServerBO, ServiceError>;
    /// 启停 MCP 服务器
    async fn toggle_mcp_server(&self, cmd: ToggleMcpCmd) -> Result<McpServerBO, ServiceError>;
    /// 删除 MCP 服务器
    async fn delete_mcp_server(&self, cmd: DeleteMcpCmd) -> Result<(), ServiceError>;
    /// MCP 市场搜索（精选 + 官方 registry；空关键字只返回精选）
    async fn search_mcp_market(
        &self,
        query: SearchMcpMarketQuery,
    ) -> Result<Vec<McpMarketItemBO>, ServiceError>;
    /// 全局权限规则列表（设置页管理）
    async fn list_global_rules(&self) -> Result<Vec<PermRuleBO>, ServiceError>;
    /// 会话可见权限规则列表（全局 + 项目 + 会话，sort 升序）
    async fn list_visible_rules(
        &self,
        session_id: i64,
        project_id: i64,
    ) -> Result<Vec<PermRuleBO>, ServiceError>;
    /// 保存权限规则（按作用域 upsert）
    async fn save_perm_rule(&self, cmd: SavePermRuleCmd) -> Result<PermRuleBO, ServiceError>;
    /// 删除权限规则
    async fn delete_perm_rule(&self, cmd: DeletePermRuleCmd) -> Result<(), ServiceError>;
}

/// 配置服务实现
pub struct ConfigServiceImpl {
    model_repo: Arc<dyn ModelRepository>,
    mcp_repo: Arc<dyn McpRepository>,
    perm_repo: Arc<dyn PermRuleRepository>,
    mcp_gateway: Arc<dyn McpGateway>,
}

impl ConfigServiceImpl {
    /// 构造
    pub fn new(
        model_repo: Arc<dyn ModelRepository>,
        mcp_repo: Arc<dyn McpRepository>,
        perm_repo: Arc<dyn PermRuleRepository>,
        mcp_gateway: Arc<dyn McpGateway>,
    ) -> Self {
        Self {
            model_repo,
            mcp_repo,
            perm_repo,
            mcp_gateway,
        }
    }

    /// 脱敏 key（keychain 读取失败时返回全掩码）
    fn masked_key_of(model_name: &str) -> String {
        secret::load_api_key(model_name)
            .map(|k| ModelConfig::mask_key(&k))
            .unwrap_or_else(|_| "****".to_string())
    }
}

#[async_trait]
impl ConfigService for ConfigServiceImpl {
    async fn list_models(&self) -> Result<Vec<ModelBO>, ServiceError> {
        let models = self.model_repo.list().await?;
        Ok(models
            .into_iter()
            .map(|m| {
                let masked = Self::masked_key_of(&m.name);
                ModelBO::from_domain(m, masked)
            })
            .collect())
    }

    async fn save_model(&self, cmd: SaveModelCmd) -> Result<ModelBO, ServiceError> {
        let now = now_local();
        let mut model = match self.model_repo.find_by_name(&cmd.name).await? {
            Some(mut existing) => {
                existing.provider = cmd.provider.clone();
                existing.base_url = cmd.base_url.clone();
                existing.context_window = cmd.context_window;
                existing.status = if cmd.enabled {
                    ModelStatus::Enabled
                } else {
                    ModelStatus::Disabled
                };
                existing.updated_at = now;
                existing
            }
            None => {
                // 新建必须提供 API Key
                if cmd.api_key.as_deref().unwrap_or("").is_empty() {
                    return Err(ServiceError::validation("新建模型必须提供 API Key"));
                }
                let mut m = ModelConfig::new(
                    cmd.name.clone(),
                    cmd.provider.clone(),
                    cmd.base_url.clone(),
                    cmd.context_window,
                    now,
                );
                m.status = if cmd.enabled {
                    ModelStatus::Enabled
                } else {
                    ModelStatus::Disabled
                };
                m
            }
        };
        model.normalize();
        model.validate()?;
        // API Key 非空 → 写 keychain，库内只存引用串
        if let Some(key) = cmd.api_key.as_deref().filter(|k| !k.is_empty()) {
            model.api_key_ref = secret::store_api_key(&model.name, key)
                .map_err(|e| ServiceError::external(format!("写入 keychain 失败：{e}")))?;
        } else if model.api_key_ref.is_empty() {
            model.api_key_ref = ModelConfig::keychain_ref(&model.name);
        }
        if model.id == 0 {
            self.model_repo.insert(&mut model).await?;
        } else {
            self.model_repo.update(&model).await?;
        }
        let masked = Self::masked_key_of(&model.name);
        Ok(ModelBO::from_domain(model, masked))
    }

    async fn delete_model(&self, cmd: DeleteModelCmd) -> Result<(), ServiceError> {
        let model = self
            .model_repo
            .find_by_id(cmd.id)
            .await?
            .ok_or_else(|| ServiceError::not_found(format!("模型不存在：{}", cmd.id)))?;
        if model.is_default {
            return Err(ServiceError::conflict("默认模型不能删除"));
        }
        self.model_repo.soft_delete(cmd.id).await?;
        secret::delete_api_key(&model.name);
        Ok(())
    }

    async fn set_default_model(&self, cmd: SetDefaultModelCmd) -> Result<(), ServiceError> {
        let mut model = self
            .model_repo
            .find_by_id(cmd.id)
            .await?
            .ok_or_else(|| ServiceError::not_found(format!("模型不存在：{}", cmd.id)))?;
        self.model_repo.clear_default().await?;
        model.is_default = true;
        self.model_repo.update(&model).await?;
        Ok(())
    }

    async fn list_mcp_servers(&self) -> Result<Vec<McpServerBO>, ServiceError> {
        let servers = self.mcp_repo.list().await?;
        Ok(servers.into_iter().map(McpServerBO::from).collect())
    }

    async fn save_mcp_server(&self, cmd: SaveMcpCmd) -> Result<McpServerBO, ServiceError> {
        let now = now_local();
        let transport = McpTransport::parse(cmd.transport.trim());
        let mut server = match self.mcp_repo.find_by_name(&cmd.name).await? {
            Some(mut existing) => {
                existing.transport = transport;
                existing.command = cmd.command.clone();
                existing.headers = cmd.headers.clone();
                existing.updated_at = now;
                existing
            }
            None => {
                let mut s = McpServer::with_transport(cmd.name.clone(), transport, cmd.command.clone(), now);
                s.headers = cmd.headers.clone();
                s
            }
        };
        server.validate()?;
        if server.id == 0 {
            self.mcp_repo.insert(&mut server).await?;
        } else {
            self.mcp_repo.update(&server).await?;
        }
        Ok(McpServerBO::from(server))
    }

    async fn toggle_mcp_server(&self, cmd: ToggleMcpCmd) -> Result<McpServerBO, ServiceError> {
        let servers = self.mcp_repo.list().await?;
        let mut server = servers
            .into_iter()
            .find(|s| s.id == cmd.id)
            .ok_or_else(|| ServiceError::not_found(format!("MCP 服务器不存在：{}", cmd.id)))?;
        if cmd.enable {
            // 真实握手：initialize + tools/list，工具数写回 mark_connected；
            // 失败 → error 状态先落库再返回错误（前端可见 last_error）
            if let Err(e) = mcp_infra::handshake(&mut server, &self.mcp_gateway).await {
                self.mcp_repo.update(&server).await?;
                return Err(ServiceError::external(e.to_string()));
            }
        } else {
            self.mcp_gateway.disconnect(&server.name).await;
            server.disable();
        }
        self.mcp_repo.update(&server).await?;
        Ok(McpServerBO::from(server))
    }

    async fn delete_mcp_server(&self, cmd: DeleteMcpCmd) -> Result<(), ServiceError> {
        self.mcp_repo.soft_delete(cmd.id).await?;
        Ok(())
    }

    async fn search_mcp_market(
        &self,
        query: SearchMcpMarketQuery,
    ) -> Result<Vec<McpMarketItemBO>, ServiceError> {
        let keyword = query.keyword.trim().to_lowercase();
        // 空关键字：只返回精选
        let featured = mcp_registry::featured_servers();
        if keyword.is_empty() {
            return Ok(featured.into_iter().map(McpMarketItemBO::from).collect());
        }
        // 精选中名称/标题/描述匹配的排在前
        let hit = |i: &mcp_registry::McpMarketItem| {
            [&i.name, &i.title, &i.description]
                .iter()
                .any(|f| f.to_lowercase().contains(&keyword))
        };
        let featured_hits: Vec<_> = featured.iter().filter(|i| hit(i)).cloned().collect();
        // registry 结果在后；与精选同 command 的剔除（去重）
        let registry = mcp_registry::search_registry(&keyword).await?;
        let featured_cmds: std::collections::HashSet<_> = featured
            .iter()
            .filter_map(|i| i.command.clone())
            .collect();
        let merged = featured_hits.into_iter().chain(
            registry
                .into_iter()
                .filter(|i| i.command.as_ref().map(|c| !featured_cmds.contains(c)).unwrap_or(true)),
        );
        Ok(merged.map(McpMarketItemBO::from).collect())
    }

    async fn list_global_rules(&self) -> Result<Vec<PermRuleBO>, ServiceError> {
        let rules = self.perm_repo.list_global().await?;
        let mut out = Self::builtin_deny_rules();
        out.extend(rules.into_iter().map(PermRuleBO::from));
        Ok(out)
    }

    async fn list_visible_rules(
        &self,
        session_id: i64,
        project_id: i64,
    ) -> Result<Vec<PermRuleBO>, ServiceError> {
        let rules = self.perm_repo.list_visible(session_id, project_id).await?;
        let mut out = Self::builtin_deny_rules();
        out.extend(rules.into_iter().map(PermRuleBO::from));
        Ok(out)
    }

    async fn save_perm_rule(&self, cmd: SavePermRuleCmd) -> Result<PermRuleBO, ServiceError> {
        let action = PermAction::parse(&cmd.action)
            .ok_or_else(|| ServiceError::validation(format!("非法权限动作：{}", cmd.action)))?;
        let now = now_local();
        // 编辑：按 id 更新，沿用原范围；新建：按 scope 校验并定位作用域
        let mut rule = match cmd.id {
            Some(id) => match self.perm_repo.find_by_id(id).await? {
                Some(mut existing) => {
                    existing.action = action;
                    existing.sort = cmd.sort;
                    existing.updated_at = now;
                    existing
                }
                None => return Err(ServiceError::not_found(format!("权限规则不存在：{id}"))),
            },
            None => {
                let scope = RuleScope::parse(&cmd.scope)
                    .ok_or_else(|| ServiceError::validation(format!("非法规则作用域：{}", cmd.scope)))?;
                let (project_id, session_id) = match scope {
                    RuleScope::Global => (None, None),
                    RuleScope::Project => (
                        Some(cmd.project_id.ok_or_else(|| {
                            ServiceError::validation("项目级规则必须指定项目")
                        })?),
                        None,
                    ),
                    RuleScope::Session => (
                        Some(cmd.project_id.ok_or_else(|| {
                            ServiceError::validation("会话级规则必须指定项目")
                        })?),
                        Some(cmd.session_id.ok_or_else(|| {
                            ServiceError::validation("会话级规则必须指定会话")
                        })?),
                    ),
                };
                match self
                    .perm_repo
                    .find_by_tool_pattern(&cmd.tool, &cmd.pattern, project_id, session_id)
                    .await?
                {
                    Some(mut existing) => {
                        existing.action = action;
                        existing.sort = cmd.sort;
                        existing.updated_at = now;
                        existing
                    }
                    None => PermissionRule {
                        id: 0,
                        project_id,
                        session_id,
                        tool: cmd.tool.clone(),
                        pattern: cmd.pattern.clone(),
                        action,
                        sort: cmd.sort,
                        plugin_origin: None,
                        created_at: now,
                        updated_at: now,
                        deleted_at: None,
                    },
                }
            }
        };
        rule.validate()?;
        if rule.id == 0 {
            self.perm_repo.insert(&mut rule).await?;
        } else {
            self.perm_repo.update(&rule).await?;
        }
        Ok(PermRuleBO::from(rule))
    }

    async fn delete_perm_rule(&self, cmd: DeletePermRuleCmd) -> Result<(), ServiceError> {
        self.perm_repo.soft_delete(cmd.id).await?;
        Ok(())
    }
}

impl ConfigServiceImpl {
    /// 内置 deny 虚拟规则（只读展示）：来自 PermissionEngine 内置清单，负数 id、
    /// 不落库、不可编辑删除。判定优先级高于一切用户规则（见 domain/agent/permission.rs）。
    fn builtin_deny_rules() -> Vec<PermRuleBO> {
        let mut next_id = -1i64;
        let mut push = |out: &mut Vec<PermRuleBO>, tool: &str, pattern: &str| {
            out.push(PermRuleBO {
                id: next_id,
                scope: "global".into(),
                project_id: None,
                session_id: None,
                tool: tool.into(),
                pattern: pattern.into(),
                action: "deny".into(),
                sort: 0,
                builtin: true,
                deleted_at: None,
            });
            next_id -= 1;
        };
        let mut out = Vec::new();
        // 敏感文件：basename glob（对所有工具生效）
        for g in crate::domain::agent::permission::PermissionEngine::builtin_deny_file_globs() {
            push(&mut out, "*", g);
        }
        // 危险命令类别：仅 Bash，语法级判定（分段 → token 化 → 程序级判定）
        for f in crate::domain::agent::permission::PermissionEngine::builtin_deny_cmd_labels() {
            push(&mut out, "Bash", f);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::db::mcp_repo::McpRepositoryImpl;
    use crate::infra::db::model_repo::ModelRepositoryImpl;
    use crate::infra::db::perm_rule_repo::PermRuleRepositoryImpl;
    use sqlx::SqlitePool;

    fn svc(pool: &SqlitePool) -> ConfigServiceImpl {
        ConfigServiceImpl::new(
            Arc::new(ModelRepositoryImpl::new(pool.clone())),
            Arc::new(McpRepositoryImpl::new(pool.clone())),
            Arc::new(PermRuleRepositoryImpl::new(pool.clone())),
            Arc::new(crate::infra::mcp::McpPool::new()),
        )
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn toggle_mcp_enable_failure_marks_error_and_persists(pool: SqlitePool) {
        let s = svc(&pool);
        let saved = s
            .save_mcp_server(SaveMcpCmd {
                id: None,
                name: "bad".into(),
                transport: "stdio".into(),
                command: "definitely-not-a-real-binary-xyz123".into(),
                headers: "{}".into(),
            })
            .await
            .unwrap();
        let err = s
            .toggle_mcp_server(ToggleMcpCmd {
                id: saved.id,
                enable: true,
            })
            .await
            .unwrap_err();
        assert_eq!(err.code, 3000);
        assert!(err.message.contains("启动 MCP 进程失败"));
        // error 状态与原因已落库
        let listed = s.list_mcp_servers().await.unwrap();
        assert_eq!(listed[0].status, "error");
        assert!(listed[0].last_error.is_some());
        // disable：断连并复位
        let bo = s
            .toggle_mcp_server(ToggleMcpCmd {
                id: saved.id,
                enable: false,
            })
            .await
            .unwrap();
        assert_eq!(bo.status, "disabled");
    }
    #[sqlx::test(migrations = "./migrations")]
    async fn search_mcp_market_empty_keyword_returns_featured_only(pool: SqlitePool) {
        // 空关键字：不打网络，只返回精选
        let items = svc(&pool)
            .search_mcp_market(SearchMcpMarketQuery {
                keyword: String::new(),
            })
            .await
            .unwrap();
        assert_eq!(
            items.len(),
            crate::infra::mcp_registry::featured_servers().len(),
            "空关键字应返回全部精选"
        );
        assert!(items.iter().all(|i| i.source == "featured"));
        assert!(items.iter().all(|i| i.command.is_some()));
        assert!(items.iter().any(|i| i.name == "context7"));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn list_rules_injects_builtin_deny_virtual_rules(pool: SqlitePool) {
        let s = svc(&pool);
        // 全局与可见列表都注入内置 deny 虚拟规则（负数 id、builtin=true、deny）
        for listed in [
            s.list_global_rules().await.unwrap(),
            s.list_visible_rules(1, 1).await.unwrap(),
        ] {
            let builtins: Vec<_> = listed.iter().filter(|r| r.builtin).collect();
            assert_eq!(builtins.len(), 8, "内置 deny = 4 敏感文件 glob + 4 危险命令类别");
            assert!(builtins.iter().all(|r| r.id < 0 && r.action == "deny"));
            assert!(builtins.iter().all(|r| r.deleted_at.is_none()));
            // 敏感文件 glob 对所有工具生效，危险命令仅 Bash
            assert_eq!(builtins.iter().filter(|r| r.tool == "*").count(), 4);
            assert_eq!(builtins.iter().filter(|r| r.tool == "Bash").count(), 4);
            assert!(builtins.iter().any(|r| r.pattern == ".env"));
            assert!(builtins.iter().any(|r| r.pattern.contains("rm")));
        }
        // 用户规则追加在内置之后，不受影响
        let saved = s
            .save_perm_rule(SavePermRuleCmd {
                id: None,
                scope: "global".into(),
                project_id: None,
                session_id: None,
                tool: "Bash".into(),
                pattern: "cargo *".into(),
                action: "allow".into(),
                sort: 1,
            })
            .await
            .unwrap();
        let listed = s.list_global_rules().await.unwrap();
        assert_eq!(listed.len(), 9, "8 内置 deny + 1 用户规则");
        assert!(!listed.last().unwrap().builtin);
        assert_eq!(listed.last().unwrap().id, saved.id);
    }
}
