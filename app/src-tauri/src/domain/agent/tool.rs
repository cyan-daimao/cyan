//! 工具调用对象与执行端口（ToolExecutor 由 infra/tools 实现）。

use async_trait::async_trait;
use serde_json::Value;

use super::llm::ChatImage;
use super::CancellationToken;
use crate::domain::shared::ProjectPath;

/// 写类工具清单（默认 Ask，plan 模式一律 Deny）
pub const WRITE_TOOLS: &[&str] = &[
    "Edit",
    "Write",
    "MultiEdit",
    "Bash",
    "BrowserAction",
    "BrowserType",
    "ComputerAction",
];
/// 是否写类工具（MCP 注入工具 `mcp__*` 一律按写类对待，默认 Ask）
/// 浏览器工具分级：导航/快照/截图/新页面是只读采集，BrowserAction/BrowserType
/// 会真实操作页面（点击/提交）按写类审批；BrowserClose 视为只读（无副作用）
pub fn is_write_tool(tool: &str) -> bool {
    WRITE_TOOLS.contains(&tool) || tool.starts_with("mcp__")
}

/// 一次工具调用
#[derive(Debug, Clone)]
pub struct ToolCall {
    /// 调用 id（审批/事件关联键）
    pub call_id: String,
    /// 工具名（Read/Write/Edit/Bash/TodoWrite/mcp__*）
    pub tool: String,
    /// 展示与权限匹配用主参数（文件相对路径或 Bash 命令串）
    pub arg: String,
    /// 原始参数 JSON
    pub input: Value,
}

/// 提供给 LLM 的工具定义
#[derive(Debug, Clone)]
pub struct ToolSpec {
    /// 工具名
    pub name: String,
    /// 工具描述
    pub description: String,
    /// 参数 JSON Schema
    pub parameters: Value,
}

/// 工具执行状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolOutputStatus {
    /// 成功
    Ok,
    /// 失败
    Error,
}

impl ToolOutputStatus {
    /// 事件字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Error => "error",
        }
    }
}

/// 写文件前生成的 checkpoint 信息（由执行器产出，application 落库）
#[derive(Debug, Clone)]
pub struct CheckpointPayload {
    /// 变更文件（相对项目）
    pub file_path: String,
    /// git blob 引用（变更前内容）
    pub git_ref: String,
    /// 新增行数
    pub add_lines: i64,
    /// 删除行数
    pub del_lines: i64,
}

/// 工具执行结果
#[derive(Debug, Clone)]
pub struct ToolOutput {
    /// 执行状态
    pub status: ToolOutputStatus,
    /// 输出内容（文本/diff/错误描述）
    pub output: String,
    /// 备注（如 `+3 / -1`）
    pub note: Option<String>,
    /// 写类工具产生的 checkpoint（Edit/Write 成功时存在）
    pub checkpoint: Option<CheckpointPayload>,
    /// 随结果回传的图片（ComputerSnapshot 截图；runner 以 user 角色多模态注入 LLM 上下文，不落库）
    pub images: Vec<ChatImage>,
}

impl ToolOutput {
    /// 成功输出
    pub fn ok(output: impl Into<String>) -> Self {
        Self {
            status: ToolOutputStatus::Ok,
            output: output.into(),
            note: None,
            checkpoint: None,
            images: Vec::new(),
        }
    }

    /// 失败输出
    pub fn error(output: impl Into<String>) -> Self {
        Self {
            status: ToolOutputStatus::Error,
            output: output.into(),
            note: None,
            checkpoint: None,
            images: Vec::new(),
        }
    }
}

/// 工具执行端口（infra/tools 实现）
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    /// 在项目根下执行一次工具调用；执行期错误收敛为 `ToolOutput::error`。
    /// `on_output` 为增量输出回调（目前仅 Bash 逐 chunk 回调，其余工具不触发）
    async fn execute(
        &self,
        project: &ProjectPath,
        call: &ToolCall,
        cancel: CancellationToken,
        on_output: &mut (dyn FnMut(String) + Send + '_),
    ) -> ToolOutput;
}
