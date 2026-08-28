import { CheckOutlined, CloseOutlined, LoadingOutlined, MessageOutlined } from '@ant-design/icons';
import type { SessionRunFlag } from '../../stores/agentStore';
import type { SessionSummaryDTO } from '../../types';

interface SessionItemProps {
  session: SessionSummaryDTO;
  active: boolean;
  /** 运行标记：running/waiting_approval → loading；done/error → 完成/出错提示 */
  runFlag?: SessionRunFlag;
  onSelect: (id: number) => void;
  onDelete: (id: number) => void;
}

/** 会话列表项：图标 + 标题 + 右侧运行指示 + 悬停删除 */
export function SessionItem({ session, active, runFlag, onSelect, onDelete }: SessionItemProps) {
  return (
    <div
      className={`session-item${active ? ' active' : ''}`}
      onClick={() => onSelect(session.id)}
    >
      <span className="s-avatar">
        <MessageOutlined />
      </span>
      <span className="s-title" title={session.title}>
        {session.title}
      </span>
      {runFlag === 'running' || runFlag === 'waiting_approval' ? (
        <span className="s-run" title={runFlag === 'waiting_approval' ? '等待审批' : '运行中'}>
          <LoadingOutlined spin />
        </span>
      ) : runFlag === 'done' ? (
        <span className="s-done" title="任务已完成">
          <CheckOutlined />
        </span>
      ) : runFlag === 'error' ? (
        <span className="s-err" title="任务出错">
          <CloseOutlined />
        </span>
      ) : null}
      <button
        className="s-del"
        title="删除会话"
        onClick={(e) => {
          e.stopPropagation();
          onDelete(session.id);
        }}
      >
        <CloseOutlined />
      </button>
    </div>
  );
}
