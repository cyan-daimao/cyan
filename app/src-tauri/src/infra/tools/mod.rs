//! 内置工具执行器（实现 domain ToolExecutor 端口）：Read/Write/Edit/MultiEdit/Bash/TodoWrite/Grep/Glob/WebFetch。
//! Edit/Write/MultiEdit 执行前打 git checkpoint，checkpoint 信息随 ToolOutput 返回给 application 落库。

use std::time::Duration;

use async_trait::async_trait;

use crate::domain::agent::{
    CancellationToken, CheckpointPayload, ToolCall, ToolExecutor, ToolOutput,
};
use crate::domain::shared::ProjectPath;

use super::{fs, git, process, web};

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
        on_output: &mut (dyn FnMut(String) + Send + '_),
    ) -> ToolOutput {
        match call.tool.as_str() {
            "Read" => exec_read(project, call),
            "Write" => exec_write(project, call),
            "Edit" => exec_edit(project, call),
            "MultiEdit" => exec_multi_edit(project, call),
            // Bash：stdout/stderr 增量经 on_output 回调（终端式滚动）
            "Bash" => exec_bash(project, call, cancel, on_output).await,
            "Grep" => exec_grep(project, call),
            "Glob" => exec_glob(project, call),
            "WebFetch" => exec_web_fetch(call).await,
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

/// MultiEdit 单处替换
#[derive(Debug, serde::Deserialize)]
struct EditPair {
    /// 待替换字符串（须唯一匹配）
    old_string: String,
    /// 替换为
    new_string: String,
}

/// 顺序应用多处替换：每处 old_string 在应用时必须唯一匹配；
/// 任一失败返回 Err((第几处（从 1 开始）, 原因))，调用方保证不写盘
fn apply_edits(before: &str, edits: &[EditPair]) -> Result<String, (usize, String)> {
    let mut content = before.to_string();
    for (idx, edit) in edits.iter().enumerate() {
        if edit.old_string.is_empty() {
            return Err((idx + 1, "old_string 不能为空".to_string()));
        }
        let occurrences = content.matches(&edit.old_string).count();
        if occurrences == 0 {
            return Err((idx + 1, "old_string 在文件中不存在".to_string()));
        }
        if occurrences > 1 {
            return Err((
                idx + 1,
                format!("old_string 出现 {occurrences} 次，无法唯一替换"),
            ));
        }
        content = content.replacen(&edit.old_string, &edit.new_string, 1);
    }
    Ok(content)
}

fn exec_multi_edit(project: &ProjectPath, call: &ToolCall) -> ToolOutput {
    let path = match arg_str(call, "path") {
        Ok(p) => p,
        Err(e) => return ToolOutput::error(e),
    };
    let edits: Vec<EditPair> = match call.input.get("edits") {
        Some(v) => match serde_json::from_value(v.clone()) {
            Ok(e) => e,
            Err(e) => return ToolOutput::error(format!("edits 参数格式非法：{e}")),
        },
        None => return ToolOutput::error("缺少参数：edits"),
    };
    if edits.is_empty() {
        return ToolOutput::error("edits 不能为空数组");
    }
    let abs = match project.resolve(path) {
        Ok(p) => p,
        Err(e) => return ToolOutput::error(e.to_string()),
    };
    let before = match std::fs::read_to_string(&abs) {
        Ok(c) => c,
        Err(e) => return ToolOutput::error(format!("读取文件失败：{e}")),
    };
    // 全部替换先落内存，任一失败则整次不写盘
    let after = match apply_edits(&before, &edits) {
        Ok(c) => c,
        Err((idx, reason)) => {
            return ToolOutput::error(format!("第 {idx} 处替换失败：{reason}（本次未写盘）"))
        }
    };
    // 与 Edit 一致：写前打 git checkpoint
    let git_ref = match git::checkpoint(project.root(), Some(before.as_bytes())) {
        Ok(r) => r,
        Err(e) => return ToolOutput::error(format!("checkpoint 失败：{e}")),
    };
    if let Err(e) = fs::write_file_abs(&abs, &after) {
        return ToolOutput::error(e.to_string());
    }
    let (add, del) = git::diff_lines(&before, &after);
    ToolOutput {
        status: crate::domain::agent::ToolOutputStatus::Ok,
        output: format!("已编辑 {path}（{} 处替换）", edits.len()),
        note: Some(format!("+{add} / -{del}")),
        checkpoint: Some(CheckpointPayload {
            file_path: path.to_string(),
            git_ref,
            add_lines: add,
            del_lines: del,
        }),
    }
}

fn exec_grep(project: &ProjectPath, call: &ToolCall) -> ToolOutput {
    let pattern = match arg_str(call, "pattern") {
        Ok(p) => p,
        Err(e) => return ToolOutput::error(e),
    };
    let include = call.input.get("include").and_then(|v| v.as_str());
    let path = call.input.get("path").and_then(|v| v.as_str());
    match fs::grep_files(project, pattern, include, path) {
        Ok(hits) => {
            if hits.is_empty() {
                return ToolOutput::ok("无匹配结果");
            }
            let text = hits
                .iter()
                .map(|h| format!("{}:{}: {}", h.rel_path, h.line_no, h.line))
                .collect::<Vec<_>>()
                .join("\n");
            ToolOutput::ok(text)
        }
        Err(e) => ToolOutput::error(e.to_string()),
    }
}

fn exec_glob(project: &ProjectPath, call: &ToolCall) -> ToolOutput {
    let pattern = match arg_str(call, "pattern") {
        Ok(p) => p,
        Err(e) => return ToolOutput::error(e),
    };
    let path = call.input.get("path").and_then(|v| v.as_str());
    match fs::glob_files(project, pattern, path) {
        Ok(files) => {
            if files.is_empty() {
                return ToolOutput::ok("无匹配文件");
            }
            ToolOutput::ok(files.join("\n"))
        }
        Err(e) => ToolOutput::error(e.to_string()),
    }
}

async fn exec_web_fetch(call: &ToolCall) -> ToolOutput {
    let url = match arg_str(call, "url") {
        Ok(u) => u,
        Err(e) => return ToolOutput::error(e),
    };
    match web::fetch_url(url).await {
        Ok(text) => ToolOutput::ok(text),
        Err(e) => ToolOutput::error(e.to_string()),
    }
}

async fn exec_bash(
    project: &ProjectPath,
    call: &ToolCall,
    cancel: CancellationToken,
    on_output: &mut (dyn FnMut(String) + Send + '_),
) -> ToolOutput {
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
    match process::run_bash(project.root(), command, timeout, cancel, on_output).await {
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
                &mut |_: String| {},
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
                &mut |_: String| {},
            )
            .await;
        assert_eq!(out.status, crate::domain::agent::ToolOutputStatus::Ok);

        let out = executor
            .execute(&project, &call("Read", json!({"path": "a.txt"}), "a.txt"), CancellationToken::new(), &mut |_: String| {})
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
                &mut |_: String| {},
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
                &mut |_: String| {},
            )
            .await;
        assert_eq!(out.status, crate::domain::agent::ToolOutputStatus::Ok);
        assert_eq!(out.output.trim(), project.root().to_string_lossy());
    }

    #[test]
    fn apply_edits_sequential_and_unique() {
        let before = "a = 1;\nb = 2;\nc = 3;\n";
        let edits = vec![
            EditPair { old_string: "a = 1;".into(), new_string: "a = 10;".into() },
            EditPair { old_string: "c = 3;".into(), new_string: "c = 30;".into() },
        ];
        let after = apply_edits(before, &edits).unwrap();
        assert_eq!(after, "a = 10;\nb = 2;\nc = 30;\n");

        // 第二处不存在 → 报错并指出第 2 处
        let edits = vec![
            EditPair { old_string: "a = 1;".into(), new_string: "x".into() },
            EditPair { old_string: "nope".into(), new_string: "y".into() },
        ];
        let err = apply_edits(before, &edits).unwrap_err();
        assert_eq!(err.0, 2);

        // 唯一匹配约束：old_string 出现 2 次 → 失败
        let dup = "foo\nfoo\n";
        let edits = vec![EditPair { old_string: "foo".into(), new_string: "bar".into() }];
        let err = apply_edits(dup, &edits).unwrap_err();
        assert_eq!(err.0, 1);
        assert!(err.1.contains("唯一"));
    }

    #[tokio::test]
    async fn multi_edit_failure_leaves_file_untouched() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("a.txt"), "hello\nworld\n").unwrap();
        let project = ProjectPath::new(tmp.path()).unwrap();
        let out = BuiltinToolExecutor
            .execute(
                &project,
                &call(
                    "MultiEdit",
                    json!({"path": "a.txt", "edits": [
                        {"old_string": "hello", "new_string": "hi"},
                        {"old_string": "missing", "new_string": "x"}
                    ]}),
                    "a.txt",
                ),
                CancellationToken::new(),
                &mut |_: String| {},
            )
            .await;
        assert_eq!(out.status, crate::domain::agent::ToolOutputStatus::Error);
        assert!(out.output.contains("第 2 处替换失败"));
        // 整次不写盘
        assert_eq!(std::fs::read_to_string(tmp.path().join("a.txt")).unwrap(), "hello\nworld\n");

        // 全部成功 → 一次写盘 + checkpoint
        let out = BuiltinToolExecutor
            .execute(
                &project,
                &call(
                    "MultiEdit",
                    json!({"path": "a.txt", "edits": [
                        {"old_string": "hello", "new_string": "hi"},
                        {"old_string": "world", "new_string": "cyan"}
                    ]}),
                    "a.txt",
                ),
                CancellationToken::new(),
                &mut |_: String| {},
            )
            .await;
        assert_eq!(out.status, crate::domain::agent::ToolOutputStatus::Ok);
        assert!(out.checkpoint.is_some());
        assert_eq!(std::fs::read_to_string(tmp.path().join("a.txt")).unwrap(), "hi\ncyan\n");
    }

    #[tokio::test]
    async fn glob_and_grep_via_executor() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();
        std::fs::write(tmp.path().join("src/a.rs"), "fn main() {}\n").unwrap();
        std::fs::write(tmp.path().join("b.txt"), "hello main\n").unwrap();
        let project = ProjectPath::new(tmp.path()).unwrap();

        let out = BuiltinToolExecutor
            .execute(&project, &call("Glob", json!({"pattern": "**/*.rs"}), "**/*.rs"), CancellationToken::new(), &mut |_: String| {})
            .await;
        assert_eq!(out.status, crate::domain::agent::ToolOutputStatus::Ok);
        assert_eq!(out.output, "src/a.rs");

        let out = BuiltinToolExecutor
            .execute(&project, &call("Grep", json!({"pattern": "main"}), "main"), CancellationToken::new(), &mut |_: String| {})
            .await;
        assert_eq!(out.status, crate::domain::agent::ToolOutputStatus::Ok);
        assert!(out.output.contains("src/a.rs:1: fn main() {}"));
        assert!(out.output.contains("b.txt:1: hello main"));
    }

    #[tokio::test]
    async fn web_fetch_bad_url_is_error_output() {
        let tmp = tempfile::tempdir().unwrap();
        let project = ProjectPath::new(tmp.path()).unwrap();
        let out = BuiltinToolExecutor
            .execute(&project, &call("WebFetch", json!({"url": "not-a-url"}), "not-a-url"), CancellationToken::new(), &mut |_: String| {})
            .await;
        assert_eq!(out.status, crate::domain::agent::ToolOutputStatus::Error);
    }
}
