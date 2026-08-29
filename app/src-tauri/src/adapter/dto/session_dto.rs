//! 会话相关 Request / DTO。

use serde::{Deserialize, Serialize};

use crate::application::session_service::{
    CreateSessionCmd, DeleteSessionCmd, GetSessionQuery, ListSessionQuery, MessageBO,
    ProjectTokenUsageBO, ProjectTokenUsageQuery, RenameSessionCmd, RestoreSessionCmd,
    SessionBO, SessionSummaryBO,
};
use crate::infra::db::fmt_time;

/// list_sessions 请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListSessionRequest {
    /// 项目路径
    pub project_path: String,
    /// 标题关键字（可选）
    pub keyword: Option<String>,
}

impl From<ListSessionRequest> for ListSessionQuery {
    fn from(r: ListSessionRequest) -> Self {
        Self {
            project_path: r.project_path,
            keyword: r.keyword,
        }
    }
}

/// get_session 请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSessionRequest {
    /// 会话 id
    pub session_id: i64,
}

impl From<GetSessionRequest> for GetSessionQuery {
    fn from(r: GetSessionRequest) -> Self {
        Self {
            session_id: r.session_id,
        }
    }
}

/// create_session 请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionRequest {
    /// 项目路径
    pub project_path: String,
}

impl From<CreateSessionRequest> for CreateSessionCmd {
    fn from(r: CreateSessionRequest) -> Self {
        Self {
            project_path: r.project_path,
        }
    }
}

/// delete_session 请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteSessionRequest {
    /// 会话 id
    pub session_id: i64,
}

impl From<DeleteSessionRequest> for DeleteSessionCmd {
    fn from(r: DeleteSessionRequest) -> Self {
        Self {
            session_id: r.session_id,
        }
    }
}

/// restore_session 请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreSessionRequest {
    /// 会话 id
    pub id: i64,
}

impl From<RestoreSessionRequest> for RestoreSessionCmd {
    fn from(r: RestoreSessionRequest) -> Self {
        Self { id: r.id }
    }
}

/// edit_message 请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditMessageRequest {
    /// 消息 id
    pub id: i64,
    /// 新文本
    pub text: String,
}

impl From<EditMessageRequest> for crate::application::session_service::EditMessageCmd {
    fn from(r: EditMessageRequest) -> Self {
        Self {
            id: r.id,
            text: r.text,
        }
    }
}

/// set_session_model 请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetSessionModelRequest {
    /// 会话 id
    pub session_id: i64,
    /// 模型名（空串 = 清除偏好，跟随全局）
    pub model: String,
}

impl From<SetSessionModelRequest> for crate::application::session_service::SetSessionModelCmd {
    fn from(r: SetSessionModelRequest) -> Self {
        Self {
            session_id: r.session_id,
            model: r.model,
        }
    }
}

/// rename_session 请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameSessionRequest {
    /// 会话 id
    pub id: i64,
    /// 新标题（trim 后 1..=80 字符）
    pub title: String,
}

impl From<RenameSessionRequest> for RenameSessionCmd {
    fn from(r: RenameSessionRequest) -> Self {
        Self {
            id: r.id,
            title: r.title,
        }
    }
}

/// token 统计 DTO
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenStatDTO {
    /// 累计输入 token
    pub input: i64,
    /// 累计输出 token
    pub output: i64,
}

/// project_token_usage 请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTokenUsageRequest {
    /// 项目路径
    pub project_path: String,
}

impl From<ProjectTokenUsageRequest> for ProjectTokenUsageQuery {
    fn from(r: ProjectTokenUsageRequest) -> Self {
        Self {
            project_path: r.project_path,
        }
    }
}

/// 项目 token 用量 DTO
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTokenUsageDTO {
    /// 累计输入 token
    pub input_tokens: i64,
    /// 累计输出 token
    pub output_tokens: i64,
    /// 会话数
    pub session_count: i64,
}

impl From<ProjectTokenUsageBO> for ProjectTokenUsageDTO {
    fn from(bo: ProjectTokenUsageBO) -> Self {
        Self {
            input_tokens: bo.input_tokens,
            output_tokens: bo.output_tokens,
            session_count: bo.session_count,
        }
    }
}

/// 会话摘要 DTO（列表项）
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummaryDTO {
    /// 会话 id
    pub id: i64,
    /// 标题
    pub title: String,
    /// 时间分组（今天/昨天/本周/更早）
    pub group: String,
    /// 上下文占用百分比
    pub ctx: i64,
    /// token 统计
    pub tokens: TokenStatDTO,
    /// 创建时间（YYYY-MM-DD HH:MM:SS）
    pub created_at: String,
    /// 更新时间
    pub updated_at: String,
}

/// 按本地日期分组
fn time_group(t: &chrono::NaiveDateTime) -> String {
    let today = chrono::Local::now().date_naive();
    let date = t.date();
    let days = (today - date).num_days();
    match days {
        0 => "今天",
        1 => "昨天",
        2..=6 => "本周",
        _ => "更早",
    }
    .to_string()
}

impl From<SessionSummaryBO> for SessionSummaryDTO {
    fn from(bo: SessionSummaryBO) -> Self {
        Self {
            id: bo.id,
            title: bo.title,
            group: time_group(&bo.updated_at),
            ctx: bo.ctx_percent,
            tokens: TokenStatDTO {
                input: bo.input_tokens,
                output: bo.output_tokens,
            },
            created_at: fmt_time(&bo.created_at),
            updated_at: fmt_time(&bo.updated_at),
        }
    }
}

/// 消息 DTO
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageDTO {
    /// 消息 id
    pub id: i64,
    /// 会话内序号
    pub seq: i64,
    /// 消息类型
    pub kind: String,
    /// 载荷（JSON 结构，前端按 kind 渲染）
    pub payload: serde_json::Value,
    /// 创建时间
    pub created_at: String,
}

impl From<MessageBO> for MessageDTO {
    fn from(bo: MessageBO) -> Self {
        Self {
            id: bo.id,
            seq: bo.seq,
            kind: bo.kind,
            payload: serde_json::from_str(&bo.payload).unwrap_or(serde_json::Value::Null),
            created_at: fmt_time(&bo.created_at),
        }
    }
}

/// 会话详情 DTO
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionDTO {
    /// 会话 id
    pub id: i64,
    /// 所属项目 id
    pub project_id: i64,
    /// 标题
    pub title: String,
    /// 上下文占用百分比
    pub ctx: i64,
    /// token 统计
    pub tokens: TokenStatDTO,
    /// 消息列表
    pub messages: Vec<MessageDTO>,
    /// 所属项目名称（回收站列表携带；常规打开为空串）
    pub project_name: String,
    /// 所属项目路径（同上）
    pub project_path: String,
    /// 会话级模型偏好（null = 跟随全局默认模型）
    pub preferred_model: Option<String>,
    /// 创建时间
    pub created_at: String,
    /// 更新时间
    pub updated_at: String,
    /// 删除时间（未删除为 null，回收站展示用）
    pub deleted_at: Option<String>,
}

impl From<SessionBO> for SessionDTO {
    fn from(bo: SessionBO) -> Self {
        Self {
            id: bo.id,
            project_id: bo.project_id,
            title: bo.title,
            ctx: bo.ctx_percent,
            tokens: TokenStatDTO {
                input: bo.input_tokens,
                output: bo.output_tokens,
            },
            messages: bo.messages.into_iter().map(MessageDTO::from).collect(),
            project_name: bo.project_name,
            project_path: bo.project_path,
            preferred_model: bo.preferred_model,
            created_at: fmt_time(&bo.created_at),
            updated_at: fmt_time(&bo.updated_at),
            deleted_at: bo.deleted_at.as_ref().map(fmt_time),
        }
    }
}
