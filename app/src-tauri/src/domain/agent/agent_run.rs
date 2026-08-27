//! AgentRun：运行状态机（idle/running/waiting_approval）+ 悬置审批管理。

use std::collections::HashMap;
use std::sync::Mutex;

use tokio::sync::oneshot;

use crate::domain::DomainError;

use super::{ApprovalDecision, CancellationToken, ToolCall};

/// 运行状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunState {
    /// 空闲
    Idle,
    /// 运行中
    Running,
    /// 等待审批
    WaitingApproval,
}

/// 悬置审批（oneshot sender + 工具上下文，用于「总是允许」规则推导）
#[derive(Debug)]
pub struct PendingApproval {
    /// 工具名
    pub tool: String,
    /// 工具目标（审批规则推导用）
    pub arg: String,
    /// 决断投递通道
    pub sender: oneshot::Sender<ApprovalDecision>,
}

/// Agent 运行（内存态，不持久化）
#[derive(Debug)]
pub struct AgentRun {
    /// 所属会话 id
    session_id: i64,
    /// 状态机
    state: Mutex<RunState>,
    /// 中断令牌
    token: CancellationToken,
    /// 悬置审批表（callId → PendingApproval）
    pending: Mutex<HashMap<String, PendingApproval>>,
}

impl AgentRun {
    /// 新建（idle）
    pub fn new(session_id: i64) -> Self {
        Self {
            session_id,
            state: Mutex::new(RunState::Idle),
            token: CancellationToken::new(),
            pending: Mutex::new(HashMap::new()),
        }
    }

    /// 会话 id
    pub fn session_id(&self) -> i64 {
        self.session_id
    }

    /// 当前状态
    pub fn state(&self) -> RunState {
        *self.state.lock().expect("run state 锁中毒")
    }

    /// 中断令牌
    pub fn token(&self) -> CancellationToken {
        self.token.clone()
    }

    /// idle → running；非 idle 启动视为运行冲突
    pub fn start(&self) -> Result<(), DomainError> {
        let mut state = self.state.lock().expect("run state 锁中毒");
        if *state != RunState::Idle {
            return Err(DomainError::Conflict("当前会话已有运行中的任务".into()));
        }
        *state = RunState::Running;
        Ok(())
    }

    /// 注册悬置审批：running → waiting_approval，返回决断接收端
    pub fn request_approval(
        &self,
        call: &ToolCall,
    ) -> Result<oneshot::Receiver<ApprovalDecision>, DomainError> {
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().expect("pending 锁中毒");
            pending.insert(
                call.call_id.clone(),
                PendingApproval {
                    tool: call.tool.clone(),
                    arg: call.arg.clone(),
                    sender: tx,
                },
            );
        }
        let mut state = self.state.lock().expect("run state 锁中毒");
        if *state != RunState::Running {
            // 状态异常（如已 interrupt），回收悬置审批
            self.pending
                .lock()
                .expect("pending 锁中毒")
                .remove(&call.call_id);
            return Err(DomainError::State("运行已结束，无法发起审批".into()));
        }
        *state = RunState::WaitingApproval;
        Ok(rx)
    }

    /// 投递审批决断：waiting_approval → running；返回 (tool, arg) 供规则推导。
    /// callId 不存在（已决断/已中断）时返回 None，保证 approve 幂等。
    pub fn approve(&self, call_id: &str, decision: ApprovalDecision) -> Option<(String, String)> {
        let entry = self
            .pending
            .lock()
            .expect("pending 锁中毒")
            .remove(call_id)?;
        let _ = entry.sender.send(decision);
        let mut state = self.state.lock().expect("run state 锁中毒");
        if *state == RunState::WaitingApproval {
            *state = RunState::Running;
        }
        Some((entry.tool, entry.arg))
    }

    /// 中断：令牌取消，悬置审批统一以 abort 决断，回到 idle
    pub fn interrupt(&self) {
        self.token.cancel();
        self.drain_pending(ApprovalDecision::Abort);
        *self.state.lock().expect("run state 锁中毒") = RunState::Idle;
    }

    /// 正常结束：回到 idle，残留悬置审批以 abort 决断
    pub fn finish(&self) {
        self.drain_pending(ApprovalDecision::Abort);
        *self.state.lock().expect("run state 锁中毒") = RunState::Idle;
    }

    /// 清空悬置审批并统一投递决断
    fn drain_pending(&self, decision: ApprovalDecision) {
        let drained: Vec<PendingApproval> = self
            .pending
            .lock()
            .expect("pending 锁中毒")
            .drain()
            .map(|(_, v)| v)
            .collect();
        for p in drained {
            let _ = p.sender.send(decision);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn call(id: &str) -> ToolCall {
        ToolCall {
            call_id: id.into(),
            tool: "Edit".into(),
            arg: "src/a.rs".into(),
            input: json!({"path": "src/a.rs"}),
        }
    }

    #[test]
    fn start_conflict_when_running() {
        let run = AgentRun::new(1);
        run.start().unwrap();
        assert!(matches!(run.start(), Err(DomainError::Conflict(_))));
    }

    #[tokio::test]
    async fn approve_resumes_and_returns_context() {
        let run = AgentRun::new(1);
        run.start().unwrap();
        let rx = run.request_approval(&call("c1")).unwrap();
        assert_eq!(run.state(), RunState::WaitingApproval);
        let ctx = run.approve("c1", ApprovalDecision::Once);
        assert_eq!(ctx, Some(("Edit".to_string(), "src/a.rs".to_string())));
        assert_eq!(run.state(), RunState::Running);
        assert_eq!(rx.await.unwrap(), ApprovalDecision::Once);
        // 重复审批幂等：callId 已不存在
        assert!(run.approve("c1", ApprovalDecision::Reject).is_none());
    }

    #[tokio::test]
    async fn interrupt_aborts_pending_approval() {
        let run = AgentRun::new(1);
        run.start().unwrap();
        let rx = run.request_approval(&call("c1")).unwrap();
        run.interrupt();
        assert_eq!(run.state(), RunState::Idle);
        assert!(run.token().is_cancelled());
        assert_eq!(rx.await.unwrap(), ApprovalDecision::Abort);
    }

    #[tokio::test]
    async fn request_approval_rejected_when_not_running() {
        let run = AgentRun::new(1);
        assert!(run.request_approval(&call("c1")).is_err());
    }
}
