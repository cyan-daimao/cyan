//! CancellationToken：统一中断令牌（SSE 流、子进程、悬置审批共用）。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio::sync::Notify;

/// 取消令牌：clonable，cancel 后所有等待点同时唤醒
#[derive(Debug, Clone, Default)]
pub struct CancellationToken {
    inner: Arc<Inner>,
}

#[derive(Debug, Default)]
struct Inner {
    cancelled: AtomicBool,
    notify: Notify,
}

impl CancellationToken {
    /// 新建
    pub fn new() -> Self {
        Self::default()
    }

    /// 触发取消（幂等）
    pub fn cancel(&self) {
        self.inner.cancelled.store(true, Ordering::SeqCst);
        self.inner.notify.notify_waiters();
    }

    /// 是否已取消
    pub fn is_cancelled(&self) -> bool {
        self.inner.cancelled.load(Ordering::SeqCst)
    }

    /// 等待取消（已取消则立即返回）
    pub async fn cancelled(&self) {
        if self.is_cancelled() {
            return;
        }
        // notified() 先注册再检查，避免 cancel 落在检查与等待之间导致漏唤醒
        let notified = self.inner.notify.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        if self.is_cancelled() {
            return;
        }
        notified.await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cancel_wakes_waiters() {
        let token = CancellationToken::new();
        let t2 = token.clone();
        let handle = tokio::spawn(async move {
            t2.cancelled().await;
            t2.is_cancelled()
        });
        tokio::task::yield_now().await;
        token.cancel();
        assert!(handle.await.unwrap());
        // 幂等
        token.cancel();
        assert!(token.is_cancelled());
    }
}
