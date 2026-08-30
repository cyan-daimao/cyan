//! Bash 执行：tokio::process + 超时 + CancellationToken kill + 审计日志。

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use tokio::io::AsyncReadExt;

use crate::domain::agent::CancellationToken;

use super::db::datasource::cyan_home;

/// 默认 Bash 超时（2 分钟）
pub const DEFAULT_BASH_TIMEOUT: Duration = Duration::from_secs(120);

/// 返回工具搜索目录（去重）：现有 PATH + HOME 下常见安装位置 + 系统级目录。
/// GUI 应用（Dock/Finder/launchd 启动）的进程 PATH 只含系统目录
/// （/usr/bin:/bin:/usr/sbin:/sbin），npx/node/uvx/cargo 等用户安装的工具解析不到。
pub fn path_search_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut push_dir = |d: PathBuf| {
        if !d.as_os_str().is_empty() && !dirs.contains(&d) {
            dirs.push(d);
        }
    };
    if let Ok(path) = std::env::var("PATH") {
        for d in std::env::split_paths(&path) {
            push_dir(d);
        }
    }
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        push_dir(home.join(".local/bin"));
        // nvm：取字典序最大的 node 版本目录（绝大多数场景即最新版）
        let nvm = home.join(".nvm/versions/node");
        if let Ok(rd) = std::fs::read_dir(&nvm) {
            let mut vers: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
            vers.sort();
            if let Some(latest) = vers.pop() {
                push_dir(latest.join("bin"));
            }
        }
        push_dir(home.join(".cargo/bin"));
        push_dir(home.join(".bun/bin"));
        push_dir(home.join(".volta/bin"));
    }
    #[cfg(target_os = "macos")]
    {
        push_dir(PathBuf::from("/opt/homebrew/bin"));
        push_dir(PathBuf::from("/usr/local/bin"));
    }
    #[cfg(not(target_os = "macos"))]
    {
        push_dir(PathBuf::from("/usr/local/bin"));
    }
    dirs
}

/// 补全后的 PATH：spawn 子进程时用（`Command::env("PATH", extended_path())`），
/// 让 `npx`/`uvx` 等脚本的 shebang（`env node`）也能解析到用户安装的解释器。
pub fn extended_path() -> std::ffi::OsString {
    std::env::join_paths(path_search_dirs())
        .unwrap_or_else(|_| std::env::var_os("PATH").unwrap_or_default())
}

/// 在搜索目录里解析程序名 → 绝对路径；含路径分隔符或解析不到时原样返回（保持 spawn 报错可读）。
pub fn resolve_program(prog: &str) -> String {
    if prog.is_empty() || prog.contains('/') {
        return prog.to_string();
    }
    for dir in path_search_dirs() {
        let cand = dir.join(prog);
        if cand.is_file() {
            return cand.to_string_lossy().into_owned();
        }
    }
    prog.to_string()
}

/// Bash 执行结果
#[derive(Debug, Clone)]
pub struct BashOutput {
    /// 标准输出
    pub stdout: String,
    /// 标准错误
    pub stderr: String,
    /// 退出码（被 kill/超时为 None）
    pub exit_code: Option<i32>,
    /// 是否超时被杀
    pub timed_out: bool,
    /// 是否被中断令牌取消
    pub cancelled: bool,
}

/// 执行 Bash 命令：cwd 为项目根；超时或 cancel 时 kill 子进程
pub async fn run_bash(
    root: &Path,
    command: &str,
    timeout: Duration,
    cancel: CancellationToken,
) -> anyhow::Result<BashOutput> {
    let start = Instant::now();
    let mut child = tokio::process::Command::new("bash")
        .arg("-c")
        .arg(command)
        .current_dir(root)
        .env("PATH", extended_path())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;

    // stdout/stderr 交给读取任务，主任务只等退出/超时/取消，保证能拿到 &mut child 做 kill
    let mut stdout_pipe = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("无法捕获子进程 stdout"))?;
    let mut stderr_pipe = child
        .stderr
        .take()
        .ok_or_else(|| anyhow::anyhow!("无法捕获子进程 stderr"))?;
    let stdout_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        let _ = stdout_pipe.read_to_end(&mut buf).await;
        buf
    });
    let stderr_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        let _ = stderr_pipe.read_to_end(&mut buf).await;
        buf
    });

    enum Ended {
        Done(Option<i32>),
        Timeout,
        Cancelled,
    }
    let ended = tokio::select! {
        status = child.wait() => Ended::Done(status?.code()),
        _ = tokio::time::sleep(timeout) => Ended::Timeout,
        _ = cancel.cancelled() => Ended::Cancelled,
    };

    let (exit_code, timed_out, cancelled, stderr_note) = match ended {
        Ended::Done(code) => (code, false, false, None),
        Ended::Timeout => {
            let _ = child.start_kill();
            let _ = child.wait().await;
            (None, true, false, Some(format!("命令超时（{}s）已被终止", timeout.as_secs())))
        }
        Ended::Cancelled => {
            let _ = child.start_kill();
            let _ = child.wait().await;
            (None, false, true, Some("任务已中断，子进程已终止".to_string()))
        }
    };

    let stdout = String::from_utf8_lossy(&stdout_task.await.unwrap_or_default()).into_owned();
    let mut stderr = String::from_utf8_lossy(&stderr_task.await.unwrap_or_default()).into_owned();
    if let Some(note) = stderr_note {
        stderr = note;
    }

    let result = BashOutput {
        stdout,
        stderr,
        exit_code,
        timed_out,
        cancelled,
    };
    // 审计日志（TECH_DESIGN 6.6：命令、决断、退出码、耗时）
    audit_log(command, &result, start.elapsed()).await;
    Ok(result)
}

/// 追加审计日志到 `~/.cyan/logs/audit.log`
async fn audit_log(command: &str, result: &BashOutput, elapsed: Duration) {
    let Ok(dir) = cyan_home().map(|h| h.join("logs")) else {
        return;
    };
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let line = format!(
        "{} | cmd={:?} | exit={:?} | timed_out={} | cancelled={} | elapsed={}ms\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        command,
        result.exit_code,
        result.timed_out,
        result.cancelled,
        elapsed.as_millis()
    );
    let path = dir.join("audit.log");
    let mut opts = std::fs::OpenOptions::new();
    let opts = opts.create(true).append(true);
    if let Ok(mut f) = opts.open(path) {
        use std::io::Write;
        let _ = f.write_all(line.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_program_finds_known_and_keeps_unknown() {
        // bash 在 macOS/Linux 系统目录必然存在 → 解析为绝对路径
        let r = resolve_program("bash");
        assert!(r.starts_with('/') && r.ends_with("bash"), "应解析为绝对路径：{r}");
        // 未知程序原样返回（spawn 报错仍可读）；含路径分隔符的不改写
        assert_eq!(resolve_program("definitely-not-a-bin-xyz"), "definitely-not-a-bin-xyz");
        assert_eq!(resolve_program("./rel/tool"), "./rel/tool");
        assert_eq!(resolve_program(""), "");
    }

    #[test]
    fn extended_path_contains_tool_dirs() {
        let dirs = path_search_dirs();
        assert!(!dirs.is_empty());
        // 去重：无重复目录
        let mut seen = dirs.clone();
        seen.sort();
        seen.dedup();
        assert_eq!(dirs.len(), seen.len(), "搜索目录不应重复");
        // extended_path 可被 join_paths 消费（非空且含分隔符语义）
        assert!(!extended_path().is_empty());
    }

    #[tokio::test]
    async fn run_echo() {
        let tmp = tempfile::tempdir().unwrap();
        let out = run_bash(
            tmp.path(),
            "echo hello",
            Duration::from_secs(10),
            CancellationToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(out.stdout.trim(), "hello");
        assert_eq!(out.exit_code, Some(0));
    }

    #[tokio::test]
    async fn run_timeout_kills_child() {
        let tmp = tempfile::tempdir().unwrap();
        let out = run_bash(
            tmp.path(),
            "sleep 30",
            Duration::from_millis(200),
            CancellationToken::new(),
        )
        .await
        .unwrap();
        assert!(out.timed_out);
        assert!(out.exit_code.is_none());
    }

    #[tokio::test]
    async fn cancel_kills_child() {
        let tmp = tempfile::tempdir().unwrap();
        let token = CancellationToken::new();
        let t2 = token.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            t2.cancel();
        });
        let out = run_bash(tmp.path(), "sleep 30", Duration::from_secs(60), token)
            .await
            .unwrap();
        assert!(out.cancelled);
    }
}
