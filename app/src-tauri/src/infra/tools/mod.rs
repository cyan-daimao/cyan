//! 内置工具执行器（实现 domain ToolExecutor 端口）：Read/Write/Edit/Bash/TodoWrite。
//! Edit/Write 执行前打 git checkpoint，checkpoint 信息随 ToolOutput 返回给 application 落库。

use std::time::Duration;

use async_trait::async_trait;

use crate::domain::agent::{
    CancellationToken, CheckpointPayload, ToolCall, ToolExecutor, ToolOutput,
};
use crate::domain::shared::ProjectPath;

use super::{fs, git, process};

/// 单 Bash 命令超时上限（10 分钟）
const MAX_BASH_TIMEOUT: Duration = Duration::from_secs(600);

/// 内置工具执行器
pub struct BuiltinToolExecutor;

#[async_trait]
impl ToolExecutor for BuiltinToolExecutor {
    async fn execute(
        &self,
        project: &ProjectPath,
        call: &ToolCall,
        cancel: CancellationToken,
    ) -> ToolOutput {
        match call.tool.as_str() {
            "Read" => exec_read(project, call),
            "Write" => exec_write(project, call),
            "Edit" => exec_edit(project, call),
            "Bash" => exec_bash(project, call, cancel).await,
            "TodoWrite" => ToolOutput::ok("todo list updated"),
            other => ToolOutput::error(format!("未知工具：{other}")),
        }
    }
}

/// 从 input 取字符串参数
fn arg_str<'a>(call: &'a ToolCall, key: &str) -> Result<&'a str, String> {
    call.input
        .get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("缺少参数：{key}"))
}

fn exec_read(project: &ProjectPath, call: &ToolCall) -> ToolOutput {
    let path = match arg_str(call, "path") {
        Ok(p) => p,
        Err(e) => return ToolOutput::error(e),
    };
    match fs::read_text_file(project, path) {
        Ok(content) => ToolOutput::ok(content),
        Err(e) => ToolOutput::error(e.to_string()),
    }
}

fn exec_write(project: &ProjectPath, call: &ToolCall) -> ToolOutput {
    let path = match arg_str(call, "path") {
        Ok(p) => p,
        Err(e) => return ToolOutput::error(e),
    };
    let content = call
        .input
        .get("content")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let abs = match project.resolve(path) {
        Ok(p) => p,
        Err(e) => return ToolOutput::error(e.to_string()),
    };
    let before = std::fs::read(&abs).ok();
    // Edit/Write 执行前打 git checkpoint（TECH_DESIGN 6.3）
    let git_ref = match git::checkpoint(project.root(), before.as_deref()) {
        Ok(r) => r,
        Err(e) => return ToolOutput::error(format!("checkpoint 失败：{e}")),
    };
    if let Err(e) = fs::write_file_abs(&abs, &content) {
        return ToolOutput::error(e.to_string());
    }
    let before_text = before
        .as_ref()
        .map(|b| String::from_utf8_lossy(b).into_owned())
        .unwrap_or_default();
    let (add, del) = git::diff_lines(&before_text, &content);
    ToolOutput {
        status: crate::domain::agent::ToolOutputStatus::Ok,
        output: format!("已写入 {path}"),
        note: Some(format!("+{add} / -{del}")),
        checkpoint: Some(CheckpointPayload {
            file_path: path.to_string(),
            git_ref,
            add_lines: add,
            del_lines: del,
        }),
    }
}

fn exec_edit(project: &ProjectPath, call: &ToolCall) -> ToolOutput {
    let path = match arg_str(call, "path") {
        Ok(p) => p,
        Err(e) => return ToolOutput::error(e),
    };
    let old_string = match arg_str(call, "old_string") {
        Ok(p) => p.to_string(),
        Err(e) => return ToolOutput::error(e),
    };
    let new_string = call
        .input
        .get("new_string")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let abs = match project.resolve(path) {
        Ok(p) => p,
        Err(e) => return ToolOutput::error(e.to_string()),
    };
    let before = match std::fs::read_to_string(&abs) {
        Ok(c) => c,
        Err(e) => return ToolOutput::error(format!("读取文件失败：{e}")),
    };
    let occurrences = before.matches(&old_string).count();
    if occurrences == 0 {
        return ToolOutput::error("old_string 在文件中不存在");
    }
    if occurrences > 1 {
        return ToolOutput::error(format!("old_string 出现 {occurrences} 次，无法唯一替换"));
    }
    // Edit 执行前打 git checkpoint
    let git_ref = match git::checkpoint(project.root(), Some(before.as_bytes())) {
        Ok(r) => r,
        Err(e) => return ToolOutput::error(format!("checkpoint 失败：{e}")),
    };
    let after = before.replacen(&old_string, &new_string, 1);
    if let Err(e) = fs::write_file_abs(&abs, &after) {
        return ToolOutput::error(e.to_string());
    }
    let (add, del) = git::diff_lines(&before, &after);
    ToolOutput {
        status: crate::domain::agent::ToolOutputStatus::Ok,
        output: format!("已编辑 {path}"),
        note: Some(format!("+{add} / -{del}")),
        checkpoint: Some(CheckpointPayload {
            file_path: path.to_string(),
            git_ref,
            add_lines: add,
            del_lines: del,
        }),
    }
}

async fn exec_bash(project: &ProjectPath, call: &ToolCall, cancel: CancellationToken) -> ToolOutput {
    let command = match arg_str(call, "command") {
        Ok(p) => p,
        Err(e) => return ToolOutput::error(e),
    };
    let timeout = call
        .input
        .get("timeout_secs")
        .and_then(|v| v.as_u64())
        .map(Duration::from_secs)
        .map(|d| d.min(MAX_BASH_TIMEOUT))
        .unwrap_or(process::DEFAULT_BASH_TIMEOUT);
    match process::run_bash(project.root(), command, timeout, cancel).await {
        Ok(out) => {
            let mut text = out.stdout;
            if !out.stderr.is_empty() {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(&out.stderr);
            }
            if out.cancelled {
                ToolOutput::error(if text.is_empty() { "已中断".into() } else { text })
            } else if out.timed_out || out.exit_code != Some(0) {
                ToolOutput::error(text)
            } else {
                ToolOutput::ok(text)
            }
        }
        Err(e) => ToolOutput::error(format!("命令执行失败：{e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn call(tool: &str, input: serde_json::Value, arg: &str) -> ToolCall {
        ToolCall {
            call_id: "c1".into(),
            tool: tool.into(),
            arg: arg.into(),
            input,
        }
    }

    #[tokio::test]
    async fn write_then_edit_then_read() {
        let tmp = tempfile::tempdir().unwrap();
        let project = ProjectPath::new(tmp.path()).unwrap();
        let executor = BuiltinToolExecutor;

        let out = executor
            .execute(
                &project,
                &call("Write", json!({"path": "a.txt", "content": "hello\nworld\n"}), "a.txt"),
                CancellationToken::new(),
            )
            .await;
        assert_eq!(out.status, crate::domain::agent::ToolOutputStatus::Ok);
        let cp = out.checkpoint.expect("Write 应产生 checkpoint");
        assert_eq!(cp.add_lines, 2);
        assert!(!cp.git_ref.is_empty());

        let out = executor
            .execute(
                &project,
                &call("Edit", json!({"path": "a.txt", "old_string": "world", "new_string": "cyan"}), "a.txt"),
                CancellationToken::new(),
            )
            .await;
        assert_eq!(out.status, crate::domain::agent::ToolOutputStatus::Ok);

        let out = executor
            .execute(&project, &call("Read", json!({"path": "a.txt"}), "a.txt"), CancellationToken::new())
            .await;
        assert_eq!(out.output, "hello\ncyan\n");
    }

    #[tokio::test]
    async fn write_escape_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let project = ProjectPath::new(tmp.path()).unwrap();
        let out = BuiltinToolExecutor
            .execute(
                &project,
                &call("Write", json!({"path": "../evil.txt", "content": "x"}), "../evil.txt"),
                CancellationToken::new(),
            )
            .await;
        assert_eq!(out.status, crate::domain::agent::ToolOutputStatus::Error);
    }

    #[tokio::test]
    async fn bash_runs_in_project_root() {
        let tmp = tempfile::tempdir().unwrap();
        let project = ProjectPath::new(tmp.path()).unwrap();
        let out = BuiltinToolExecutor
            .execute(
                &project,
                &call("Bash", json!({"command": "pwd"}), "pwd"),
                CancellationToken::new(),
            )
            .await;
        assert_eq!(out.status, crate::domain::agent::ToolOutputStatus::Ok);
        assert_eq!(out.output.trim(), project.root().to_string_lossy());
    }
}
