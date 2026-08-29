//! Sidecar 后端管理端口（infra/sidecar 实现）：插件开关控制外部进程启停。

use async_trait::async_trait;

/// sidecar 启动结果
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SidecarInfo {
    /// 分配端口
    pub port: u16,
    /// 子进程 pid
    pub pid: u32,
}

/// sidecar 实时状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SidecarStatus {
    /// 是否运行中
    pub running: bool,
    /// 占用端口
    pub port: Option<u16>,
}

/// sidecar 管理端口（进程注册表 + 端口分配器）
#[async_trait]
pub trait SidecarGateway: Send + Sync {
    /// 启动 sidecar：分配端口 → 替换 `{port}` spawn → 健康检查；已在运行则幂等返回现状。
    /// 失败时内部已 kill 子进程并释放端口。
    async fn start(
        &self,
        plugin: &str,
        plugin_dir: &std::path::Path,
        command_tpl: &str,
        health_path: Option<&str>,
    ) -> anyhow::Result<SidecarInfo>;
    /// 停止 sidecar 并释放端口（幂等）
    async fn stop(&self, plugin: &str);
    /// 查询实时状态
    fn status(&self, plugin: &str) -> SidecarStatus;
    /// 停止全部（App 退出回收）
    async fn stop_all(&self);
}
