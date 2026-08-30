//! Sidecar 管理器：受管端口段分配 + 子进程启停 + 健康检查轮询。
//! 内存注册表 plugin→(port, child)；kill_on_drop 兜底，App 退出经 stop_all 回收。

use std::collections::HashMap;
use std::net::TcpListener;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;

use crate::domain::plugin::{SidecarGateway, SidecarInfo, SidecarStatus};

/// 受管端口段（18700–18799）
const PORT_RANGE: std::ops::RangeInclusive<u16> = 18700..=18799;

/// 健康检查总超时（15s）与轮询间隔（500ms）
const HEALTH_TIMEOUT: Duration = Duration::from_secs(15);
const HEALTH_INTERVAL: Duration = Duration::from_millis(500);

/// 注册表条目
struct SidecarEntry {
    /// 分配端口
    port: u16,
    /// 子进程（stop 时显式 kill）
    child: tokio::process::Child,
}

/// sidecar 管理器（实现 domain SidecarGateway 端口）
pub struct SidecarManager {
    entries: Mutex<HashMap<String, SidecarEntry>>,
    /// 健康检查超时（测试可缩短）
    health_timeout: Duration,
}

impl SidecarManager {
    /// 构造
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            health_timeout: HEALTH_TIMEOUT,
        }
    }

    /// 构造（自定义健康检查超时，测试用）
    pub fn with_health_timeout(health_timeout: Duration) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            health_timeout,
        }
    }

    /// 分配端口：先查内存（同插件复用），再按段探测空闲；段耗尽报错
    fn allocate(&self, plugin: &str) -> anyhow::Result<u16> {
        let entries = self.entries.lock().expect("sidecar 锁中毒");
        if let Some(e) = entries.get(plugin) {
            return Ok(e.port);
        }
        for port in PORT_RANGE {
            if entries.values().any(|e| e.port == port) {
                continue;
            }
            // 探测空闲（bind 成功立即释放，spawn 由健康检查兜底竞争窗口）
            if TcpListener::bind(("127.0.0.1", port)).is_ok() {
                return Ok(port);
            }
        }
        Err(anyhow::anyhow!(
            "sidecar 端口段（{}–{}）已耗尽",
            PORT_RANGE.start(),
            PORT_RANGE.end()
        ))
    }

    /// 健康检查轮询：200 就绪；子进程提前退出立即失败
    async fn wait_ready(
        &self,
        child: &mut tokio::process::Child,
        port: u16,
        health_path: &str,
    ) -> anyhow::Result<()> {
        let url = format!("http://127.0.0.1:{port}{health_path}");
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(3))
            .build()?;
        let deadline = tokio::time::Instant::now() + self.health_timeout;
        loop {
            // 子进程提前退出 → 快速失败（不等满超时）
            if let Some(status) = child.try_wait()? {
                return Err(anyhow::anyhow!(
                    "sidecar 进程提前退出（{status}）：健康检查未通过"
                ));
            }
            if let Ok(resp) = client.get(&url).send().await {
                if resp.status().is_success() {
                    return Ok(());
                }
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(anyhow::anyhow!(
                    "sidecar 健康检查超时（{}s）：{url}",
                    self.health_timeout.as_secs()
                ));
            }
            tokio::time::sleep(HEALTH_INTERVAL).await;
        }
    }
}

impl Default for SidecarManager {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SidecarGateway for SidecarManager {
    async fn start(
        &self,
        plugin: &str,
        plugin_dir: &Path,
        command_tpl: &str,
        health_path: Option<&str>,
    ) -> anyhow::Result<SidecarInfo> {
        // 幂等：已在运行直接返回现状
        if let Some(e) = self.entries.lock().expect("sidecar 锁中毒").get(plugin) {
            return Ok(SidecarInfo {
                port: e.port,
                pid: e.child.id().unwrap_or(0),
            });
        }
        let port = self.allocate(plugin)?;
        let cmdline = command_tpl.replace("{port}", &port.to_string());
        // 空格分词（不引 shell 解析库）
        let mut parts = cmdline.split_whitespace();
        let program = parts
            .next()
            .ok_or_else(|| anyhow::anyhow!("sidecar 启动命令为空"))?;
        let mut child = tokio::process::Command::new(program)
            .args(parts)
            .current_dir(plugin_dir)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()?;
        // 健康检查（无 healthPath 则 spawn 后即就绪）
        if let Some(hp) = health_path {
            if let Err(e) = self.wait_ready(&mut child, port, hp).await {
                let _ = child.kill().await;
                return Err(e);
            }
        }
        let pid = child.id().unwrap_or(0);
        self.entries
            .lock()
            .expect("sidecar 锁中毒")
            .insert(plugin.to_string(), SidecarEntry { port, child });
        tracing::info!(plugin, port, pid, "sidecar 已启动");
        Ok(SidecarInfo { port, pid })
    }

    async fn stop(&self, plugin: &str) {
        let entry = self
            .entries
            .lock()
            .expect("sidecar 锁中毒")
            .remove(plugin);
        if let Some(mut e) = entry {
            let _ = e.child.kill().await;
            let _ = e.child.wait().await;
            tracing::info!(plugin, port = e.port, "sidecar 已停止");
        }
    }

    fn status(&self, plugin: &str) -> SidecarStatus {
        self.entries
            .lock()
            .expect("sidecar 锁中毒")
            .get(plugin)
            .map(|e| SidecarStatus {
                running: true,
                port: Some(e.port),
            })
            .unwrap_or_default()
    }

    async fn stop_all(&self) {
        let drained: Vec<(String, SidecarEntry)> = self
            .entries
            .lock()
            .expect("sidecar 锁中毒")
            .drain()
            .collect();
        for (plugin, mut e) in drained {
            let _ = e.child.kill().await;
            let _ = e.child.wait().await;
            tracing::info!(plugin, "sidecar 退出回收");
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// 进程内测试锁：真实起 HTTP 服务的测试串行化（防端口 probe/spawn 竞争）
    static HTTP_TEST_LOCK: std::sync::OnceLock<Mutex<()>> = std::sync::OnceLock::new();

    pub(crate) fn http_test_lock() -> std::sync::MutexGuard<'static, ()> {
        HTTP_TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    /// 把管理器全部端口占满（段耗尽测试）
    fn exhaust_ports(m: &SidecarManager) {
        let mut entries = m.entries.lock().unwrap();
        for port in PORT_RANGE {
            entries.insert(
                format!("fake-{port}"),
                SidecarEntry {
                    port,
                    child: tokio::process::Command::new("sleep")
                        .arg("0.1")
                        .kill_on_drop(true)
                        .spawn()
                        .unwrap(),
                },
            );
        }
    }

    #[tokio::test]
    async fn allocate_distinct_and_release_reuse() {
        let m = SidecarManager::new();
        let tmp = tempfile::tempdir().unwrap();
        // 运行中的条目占用端口
        let b = m.start("b", tmp.path(), "sleep 30", None).await.unwrap();
        // 其他插件分配不得与运行中的端口冲突
        let c = m.allocate("c").unwrap();
        assert_ne!(c, b.port);
        // stop 后端口释放，可被后续分配复用
        m.stop("b").await;
        m.stop("b").await; // 幂等
        assert!(!m.status("b").running);
        assert!(PORT_RANGE.contains(&m.allocate("d").unwrap()));
    }

    #[tokio::test]
    async fn allocate_exhaustion_err() {
        let m = SidecarManager::new();
        exhaust_ports(&m);
        let err = m.allocate("overflow").unwrap_err();
        assert!(err.to_string().contains("耗尽"));
        m.stop_all().await;
    }

    #[tokio::test]
    async fn start_stop_roundtrip_with_sleep() {
        let m = SidecarManager::new();
        let tmp = tempfile::tempdir().unwrap();
        let info = m.start("sleeper", tmp.path(), "sleep 30", None).await.unwrap();
        assert!(info.pid > 0);
        assert!(m.status("sleeper").running);
        // 幂等 start
        let again = m.start("sleeper", tmp.path(), "sleep 30", None).await.unwrap();
        assert_eq!(again.port, info.port);
        m.stop("sleeper").await;
        assert!(!m.status("sleeper").running);
    }

    #[tokio::test]
    async fn health_check_fast_fail_on_child_exit() {
        let m = SidecarManager::with_health_timeout(Duration::from_secs(30));
        let tmp = tempfile::tempdir().unwrap();
        // `false` 立即退出 → 健康检查快速失败，不等满超时
        let start = std::time::Instant::now();
        let err = m
            .start("bad", tmp.path(), "false", Some("/health"))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("提前退出"));
        assert!(start.elapsed() < Duration::from_secs(10));
        assert!(!m.status("bad").running);
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)] // 测试锁故意跨 await 持有：串行化端口分配
    async fn health_check_with_real_http_server() {
        // python3 可用性检查（CI 无 python3 时跳过）
        if std::process::Command::new("python3")
            .arg("--version")
            .output()
            .is_err()
        {
            return;
        }
        // 并行测试下不同 manager 实例可能同时探测到同一空闲端口（probe 与 spawn 之间存在竞争窗口），
        // 用进程内锁把真实 HTTP 起服务的测试串行化
        let _guard = http_test_lock();
        let m = SidecarManager::new();
        let tmp = tempfile::tempdir().unwrap();
        let info = m
            .start(
                "httpd",
                tmp.path(),
                "python3 -m http.server {port} --bind 127.0.0.1",
                Some("/"),
            )
            .await
            .unwrap();
        assert!(m.status("httpd").running);
        // 服务确实就绪
        let resp = reqwest::get(format!("http://127.0.0.1:{}/", info.port))
            .await
            .unwrap();
        assert!(resp.status().is_success());
        m.stop("httpd").await;
        assert!(!m.status("httpd").running);
    }
}
