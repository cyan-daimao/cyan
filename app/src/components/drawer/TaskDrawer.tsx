import { useState } from 'react';
import { Drawer, Modal } from 'antd';
import type { ChangeView } from '../../types';
import { useAgentStore } from '../../stores/agentStore';
import { confirmDanger } from '../../utils/feedback';
import { guardBusy } from '../../utils/guard';
import { DiffView } from '../message/DiffView';

interface TaskDrawerProps {
  open: boolean;
  onClose: () => void;
}

/** 任务与变更抽屉：TODO 清单 + 文件变更（checkpoint）列表 */
export function TaskDrawer({ open, onClose }: TaskDrawerProps) {
  const todos = useAgentStore((s) => s.todos);
  const changes = useAgentStore((s) => s.changes);
  const rollback = useAgentStore((s) => s.rollback);
  const [viewing, setViewing] = useState<ChangeView | null>(null);

  const onRollback = (c: ChangeView) => {
    // 运行中禁止回滚（PRD 8.4）
    if (guardBusy('回滚变更')) return;
    confirmDanger({
      title: '回滚变更',
      content: (
        <span>
          将 <b className="mono">{c.filePath}</b> 恢复到本 checkpoint 之前的状态？此操作不可撤销。
        </span>
      ),
      okText: '回滚',
      onOk: () => rollback(c.changeId),
    });
  };

  return (
    <>
      <Drawer title="任务与变更" placement="right" width={400} open={open} onClose={onClose}>
        <div className="section-title">✅ 当前任务（TODO）</div>
        {todos.length === 0 ? (
          <div className="drawer-empty">暂无进行中的任务</div>
        ) : (
          todos.map((t) => (
            <div
              key={t.id}
              className={`todo-item${t.status === 'done' ? ' done' : t.status === 'in_progress' ? ' doing' : ''}`}
            >
              <span className="todo-check">{t.status === 'done' ? '✓' : ''}</span>
              <span className="todo-text">{t.content}</span>
            </div>
          ))
        )}
        <div className="section-title">📝 文件变更（Checkpoints）</div>
        {changes.length === 0 ? (
          <div className="drawer-empty">本次会话还没有文件改动</div>
        ) : (
          changes.map((c) => (
            <div className="change-item" key={c.changeId}>
              <span>📄</span>
              <span className="f-path mono" title={c.filePath}>
                {c.filePath}
              </span>
              <span className="diff-stat">
                <span className="add">+{c.addLines}</span> <span className="del">-{c.delLines}</span>
              </span>
              <a onClick={() => setViewing(c)}>查看</a>
              <a style={{ color: 'var(--error)' }} onClick={() => onRollback(c)}>
                回滚
              </a>
            </div>
          ))
        )}
      </Drawer>
      <Modal
        open={viewing !== null}
        title={viewing ? `变更：${viewing.filePath}` : ''}
        width={860}
        footer={null}
        onCancel={() => setViewing(null)}
      >
        {viewing ? <DiffView diff={viewing.diff} /> : null}
      </Modal>
    </>
  );
}
