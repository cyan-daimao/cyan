import { Button, Dropdown, Tag } from 'antd';
import {
  CheckCircleOutlined,
  CloseCircleOutlined,
  DownOutlined,
  ExclamationCircleOutlined,
  ThunderboltOutlined,
} from '@ant-design/icons';
import type { ApprovalDecision, ApprovalState, RuleScope } from '../../types';

interface ApprovalCardProps {
  callId: string;
  tool: string;
  arg: string;
  reason: string;
  state: ApprovalState;
  onDecide: (callId: string, decision: ApprovalDecision, alwaysScope?: RuleScope) => void;
}

/** 审批卡：暖色内嵌会话流（允许一次 / 总是允许-按作用域 / 拒绝） */
export function ApprovalCard({ callId, tool, arg, reason, state, onDecide }: ApprovalCardProps) {
  return (
    <div className="approval-card">
      <div className="approval-title">
        <ExclamationCircleOutlined /> 需要批准：<span className="mono">{tool}</span>
      </div>
      <div className="approval-cmd mono">{arg}</div>
      <div className="approval-reason">{reason}</div>
      {state === 'pending' ? (
        <div className="approval-actions">
          <Button type="primary" size="small" onClick={() => onDecide(callId, 'once')}>
            允许一次
          </Button>
          <Dropdown
            menu={{
              items: [
                { key: 'session', label: '本会话生效' },
                { key: 'project', label: '本项目生效' },
                { key: 'global', label: '全局生效' },
              ],
              onClick: ({ key }) => onDecide(callId, 'always', key as RuleScope),
            }}
          >
            <Button size="small">
              总是允许 {tool} <DownOutlined style={{ fontSize: 10 }} />
            </Button>
          </Dropdown>
          <Button size="small" danger onClick={() => onDecide(callId, 'reject')}>
            拒绝
          </Button>
        </div>
      ) : null}
      {state === 'allowed' ? (
        <Tag icon={<CheckCircleOutlined />} color="success">
          已允许
        </Tag>
      ) : null}
      {state === 'always' ? (
        <Tag icon={<CheckCircleOutlined />} color="success">
          已加入白名单
        </Tag>
      ) : null}
      {state === 'auto' ? (
        <Tag icon={<ThunderboltOutlined />} color="processing">
          自动模式已批准
        </Tag>
      ) : null}
      {state === 'rejected' ? (
        <Tag icon={<CloseCircleOutlined />} color="error">
          已拒绝
        </Tag>
      ) : null}
    </div>
  );
}
