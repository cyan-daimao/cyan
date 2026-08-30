//! cyan lib：模块导出与 Tauri Builder 装配（main.rs 调 `cyan_lib::run()`）。

pub mod adapter;
pub mod application;
pub mod domain;
pub mod error;
pub mod infra;

use std::sync::Arc;

use tauri::Manager;

use adapter::command::{
    agent_command, config_command, file_command, plugin_command, project_command, recycle_command,
    session_command, skill_command,
};
use adapter::event::TauriEventSink;
use application::agent_service::{AgentService, AgentServiceImpl};
use application::config_service::{ConfigService, ConfigServiceImpl};
use application::plugin_service::{PluginService, PluginServiceImpl};
use application::project_service::{ProjectService, ProjectServiceImpl};
use application::recycle_service::{RecycleService, RecycleServiceImpl};
use application::session_service::{SessionService, SessionServiceImpl};
use application::skill_service::{SkillService, SkillServiceImpl};
use domain::agent::{CheckpointGateway, LlmGateway, RunEventSink, ToolExecutor};
use domain::config::{McpRepository, ModelRepository, PermRuleRepository};
use domain::plugin::{PluginRepository, SidecarGateway};
use domain::project::ProjectRepository;
use domain::session::{MessageRepository, SessionRepository};
use infra::db::checkpoint_repo::CheckpointRepositoryImpl;
use infra::db::mcp_repo::McpRepositoryImpl;
use infra::db::model_repo::ModelRepositoryImpl;
use infra::db::perm_rule_repo::PermRuleRepositoryImpl;
use infra::db::plugin_repo::PluginRepositoryImpl;
use infra::db::project_repo::ProjectRepositoryImpl;
use infra::db::recycle::RecycleBinRepositoryImpl;
use infra::db::session_repo::{MessageRepositoryImpl, SessionRepositoryImpl};
use infra::git::GitCheckpointGateway;
use infra::llm::OpenAiClient;
use infra::tools::BuiltinToolExecutor;

/// 初始化 tracing：按天滚动写 `~/.cyan/logs/cyan.log.YYYY-MM-DD`
fn init_tracing() {
    let log_dir = infra::db::datasource::cyan_home()
        .map(|h| h.join("logs"))
        .unwrap_or_else(|_| std::path::PathBuf::from("."));
    let _ = std::fs::create_dir_all(&log_dir);
    let file_appender = tracing_appender::rolling::daily(&log_dir, "cyan.log");
    let (writer, guard) = tracing_appender::non_blocking(file_appender);
    // guard 需存活整个进程：主动泄漏（进程退出时随内存回收，日志已随 drop 语义之外的 flush 写出）
    std::mem::forget(guard);
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(writer)
        .with_ansi(false)
        .init();
}

/// 启动 Tauri 应用
pub fn run() {
    // sidecar 管理器：setup 注入插件服务，退出钩子回收全部子进程
    let sidecar_manager = Arc::new(infra::sidecar::SidecarManager::new());
    let sidecar_for_setup = sidecar_manager.clone();
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| {
            init_tracing();
            tracing::info!("cyan 启动，初始化数据库");
            let pool = tauri::async_runtime::block_on(infra::db::datasource::init_pool())
                .map_err(|e| format!("数据库初始化失败：{e}"))?;

            // infra → Repository 实现
            let session_repo: Arc<dyn SessionRepository> =
                Arc::new(SessionRepositoryImpl::new(pool.clone()));
            let message_repo: Arc<dyn MessageRepository> =
                Arc::new(MessageRepositoryImpl::new(pool.clone()));
            let project_repo: Arc<dyn ProjectRepository> =
                Arc::new(ProjectRepositoryImpl::new(pool.clone()));
            let model_repo: Arc<dyn ModelRepository> =
                Arc::new(ModelRepositoryImpl::new(pool.clone()));
            let mcp_repo: Arc<dyn McpRepository> = Arc::new(McpRepositoryImpl::new(pool.clone()));
            let perm_repo: Arc<dyn PermRuleRepository> =
                Arc::new(PermRuleRepositoryImpl::new(pool.clone()));
            let checkpoint_repo = Arc::new(CheckpointRepositoryImpl::new(pool.clone()));
            let plugin_repo: Arc<dyn PluginRepository> =
                Arc::new(PluginRepositoryImpl::new(pool.clone()));

            // infra → 端口实现
            let llm: Arc<dyn LlmGateway> = Arc::new(OpenAiClient::new());
            let executor: Arc<dyn ToolExecutor> = Arc::new(BuiltinToolExecutor);
            let checkpoint_gateway: Arc<dyn CheckpointGateway> = Arc::new(GitCheckpointGateway);
            let sink: Arc<dyn RunEventSink> =
                Arc::new(TauriEventSink::new(app.handle().clone()));
            // MCP 连接池（config 握手与 agent 工具注入共享同一实例）
            let mcp_gateway: Arc<dyn infra::mcp::McpGateway> = Arc::new(infra::mcp::McpPool::new());

            // application 服务装配（Arc<dyn Service> 注入 adapter）
            let session_service: Arc<dyn SessionService> = Arc::new(SessionServiceImpl::new(
                session_repo.clone(),
                message_repo.clone(),
                project_repo.clone(),
                Arc::new(RecycleBinRepositoryImpl::new(pool.clone())),
                model_repo.clone(),
                checkpoint_repo.clone(),
            ));
            let project_service: Arc<dyn ProjectService> = Arc::new(ProjectServiceImpl::new(
                project_repo.clone(),
                session_repo.clone(),
                message_repo.clone(),
                checkpoint_repo.clone(),
                perm_repo.clone(),
            ));
            let config_service: Arc<dyn ConfigService> = Arc::new(ConfigServiceImpl::new(
                model_repo.clone(),
                mcp_repo.clone(),
                perm_repo.clone(),
                mcp_gateway.clone(),
            ));
            // 插件根目录 `~/.cyan/plugins`（测试经构造注入隔离）
            let plugins_dir = infra::db::datasource::cyan_home()
                .map(|h| h.join("plugins"))
                .unwrap_or_else(|_| std::path::PathBuf::from(".cyan/plugins"));
            let plugin_service: Arc<dyn PluginService> = Arc::new(PluginServiceImpl::new(
                plugin_repo.clone(),
                mcp_repo.clone(),
                perm_repo.clone(),
                plugins_dir.clone(),
                sidecar_for_setup.clone(),
            ));
            let skill_service: Arc<dyn SkillService> = Arc::new(SkillServiceImpl::new(
                plugin_repo.clone(),
                plugins_dir,
                infra::db::datasource::cyan_home()
                    .map(|h| h.join("skills"))
                    .unwrap_or_else(|_| std::path::PathBuf::from(".cyan/skills")),
            ));
            let recycle_service: Arc<dyn RecycleService> = Arc::new(RecycleServiceImpl::new(
                session_repo.clone(),
                message_repo.clone(),
                project_repo.clone(),
                checkpoint_repo.clone(),
                model_repo.clone(),
                mcp_repo.clone(),
                plugin_repo,
                perm_repo.clone(),
            ));
            let agent_service: Arc<dyn AgentService> = Arc::new(AgentServiceImpl::new(
                session_repo,
                message_repo,
                project_repo,
                checkpoint_repo,
                perm_repo,
                model_repo,
                llm,
                executor,
                checkpoint_gateway,
                sink,
                mcp_gateway,
            ));

            app.manage(session_service);
            app.manage(project_service);
            app.manage(config_service);
            app.manage(plugin_service);
            app.manage(skill_service);
            app.manage(recycle_service);
            app.manage(agent_service);
            tracing::info!("cyan 初始化完成");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // 会话
            session_command::list_sessions,
            session_command::get_session,
            session_command::create_session,
            session_command::delete_session,
            session_command::project_token_usage,
            // 回收站
            session_command::list_deleted_sessions,
            session_command::restore_session,
            session_command::purge_recycle_bin,
            recycle_command::list_recycle_bin,
            recycle_command::restore_recycle_item,
            // 消息编辑
            session_command::edit_message,
            session_command::set_session_model,
            session_command::rename_session,
            session_command::clear_session,
            // Agent
            agent_command::send_task,
            agent_command::interrupt_run,
            agent_command::approve,
            agent_command::rollback_change,
            // 项目
            project_command::list_projects,
            project_command::open_project,
            project_command::create_project,
            project_command::remove_project,
            // 文件
            file_command::file_tree,
            file_command::file_preview,
            // 模型配置
            config_command::list_models,
            config_command::save_model,
            config_command::delete_model,
            config_command::set_default_model,
            // MCP
            config_command::list_mcp_servers,
            config_command::save_mcp_server,
            config_command::toggle_mcp_server,
            config_command::delete_mcp_server,
            config_command::search_mcp_market,
            // 权限规则
            config_command::list_global_perm_rules,
            config_command::list_visible_perm_rules,
            config_command::save_perm_rule,
            config_command::delete_perm_rule,
            // 技能
            skill_command::list_skills,
            skill_command::save_skill,
            skill_command::delete_skill,
            skill_command::search_skill_market,
            skill_command::install_skill_from_github,
            // 插件
            plugin_command::list_plugins,
            plugin_command::install_plugin,
            plugin_command::toggle_plugin,
            plugin_command::delete_plugin,
            plugin_command::search_marketplace,
            plugin_command::install_plugin_from_github,
        ]);
    let app = builder
        .build(tauri::generate_context!())
        .expect("error while building cyan application");
    // App 退出回收：停掉全部 sidecar 子进程（kill_on_drop 之外的显式清理）
    app.run(move |_handle, event| {
        if let tauri::RunEvent::ExitRequested { .. } = event {
            tauri::async_runtime::block_on(sidecar_manager.stop_all());
        }
    });
}
