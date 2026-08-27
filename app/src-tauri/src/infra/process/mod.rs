//! Bash 执行：tokio::process + 超时 + CancellationToken kill + 审计日志。

use std::path::Path;
use std::time::{Duration, Instant};

use tokio::io::AsyncReadExt;

use crate::domain::agent::CancellationToken;

use super::db::datasource::cyan_home;

/// 默认 Bash 超时（2 分钟）
pub const DEFAULT_BASH_TIMEOUT: Duration = Duration::from_secs(120);

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
