import { Button, Tag } from 'antd';
import type { ApprovalDecision, ApprovalState } from '../../types';

interface ApprovalCardProps {
  callId: string;
  tool: string;
  arg: string;
  reason: string;
  state: ApprovalState;
  onDecide: (callId: string, decision: ApprovalDecision) => void;
}

/** 审批卡：暖色内嵌会话流（允许一次 / 总是允许 / 拒绝） */
export function ApprovalCard({ callId, tool, arg, reason, state, onDecide }: ApprovalCardProps) {
  return (
    <div className="approval-card">
      <div className="approval-title">
        ⚠️ 需要批准：<span className="mono">{tool}</span>
      </div>
      <div className="approval-cmd mono">{arg}</div>
      <div className="approval-reason">{reason}</div>
      {state === 'pending' ? (
        <div className="approval-actions">
          <Button type="primary" size="small" onClick={() => onDecide(callId, 'once')}>
            允许一次
          </Button>
          <Button size="small" onClick={() => onDecide(callId, 'always')}>
            总是允许 {tool}
          </Button>
          <Button size="small" danger onClick={() => onDecide(callId, 'reject')}>
            拒绝
          </Button>
        </div>
      ) : null}
      {state === 'allowed' ? <Tag color="success">✓ 已允许</Tag> : null}
      {state === 'always' ? <Tag color="success">✓ 已加入白名单</Tag> : null}
      {state === 'auto' ? <Tag color="processing">⚡ 自动模式已批准</Tag> : null}
      {state === 'rejected' ? <Tag color="error">✕ 已拒绝</Tag> : null}
    </div>
  );
}
