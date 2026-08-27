//! 推送到前端的事件定义与实现（`agent:event` 单通道 + type 判别）。

use tauri::{AppHandle, Emitter};

use crate::domain::agent::{AgentEvent, RunEventSink};

use super::dto::AgentEventDTO;

/// `agent:event` 通道名
pub const AGENT_EVENT_CHANNEL: &str = "agent:event";

/// 基于 AppHandle 的事件推送器（domain RunEventSink 端口的 adapter 实现）
pub struct TauriEventSink {
    app: AppHandle,
}

impl TauriEventSink {
    /// 构造
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl RunEventSink for TauriEventSink {
    fn emit(&self, event: AgentEvent) {
        let dto = AgentEventDTO::from(event);
        if let Err(e) = self.app.emit(AGENT_EVENT_CHANNEL, &dto) {
            tracing::warn!(error = %e, "事件推送失败");
        }
    }
}
