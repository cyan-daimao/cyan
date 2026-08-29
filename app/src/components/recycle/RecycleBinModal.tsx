import { useCallback, useEffect, useState } from 'react';
import { Button, Modal, Popconfirm } from 'antd';
import { DeleteOutlined, UndoOutlined } from '@ant-design/icons';
import type { SessionDTO } from '../../types';
import { listDeletedSessions, purgeRecycleBin, restoreSession } from '../../services/session';
import { useSessionStore } from '../../stores/sessionStore';
import { useProjectStore } from '../../stores/projectStore';
import { errText, toast } from '../../utils/feedback';
import { Empty } from '../common/Empty';

interface RecycleBinModalProps {
  open: boolean;
  onClose: () => void;
}

/** 回收站：已删除会话的恢复 / 清空 */
export function RecycleBinModal({ open, onClose }: RecycleBinModalProps) {
  const [items, setItems] = useState<SessionDTO[]>([]);
  const [loading, setLoading] = useState(false);
  const [restoring, setRestoring] = useState<number | null>(null);
  const [purging, setPurging] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setItems(await listDeletedSessions());
    } catch (e) {
      toast.error(`加载回收站失败：${errText(e)}`);
    } finally {
      setLoading(false);
    }
  }, []);

  // 打开时加载
  useEffect(() => {
    if (open) void load();
  }, [open, load]);

  /** 刷新侧栏项目/会话树 */
  const refreshSidebar = useCallback(() => {
    const cur = useProjectStore.getState().current;
    void useProjectStore.getState().loadRecents();
    if (cur) {
      void useSessionStore
        .getState()
        .loadSessions(cur.path, useSessionStore.getState().searchKw || undefined);
    }
  }, []);

  const onRestore = async (s: SessionDTO) => {
    setRestoring(s.id);
    try {
      await restoreSession(s.id);
      setItems((list) => list.filter((x) => x.id !== s.id));
      refreshSidebar();
      toast.success(`已恢复会话「${s.title}」`);
    } catch (e) {
      // 失败保留该行
      toast.error(`恢复失败：${errText(e)}`);
    } finally {
      setRestoring(null);
    }
  };

  const onPurge = async () => {
    setPurging(true);
    try {
      const n = await purgeRecycleBin();
      toast.success(`已清理 ${n} 条记录`);
      await load();
      refreshSidebar();
    } catch (e) {
      toast.error(`清空回收站失败：${errText(e)}`);
    } finally {
      setPurging(false);
    }
  };

  return (
    <Modal
      open={open}
      title="回收站"
      width={640}
      footer={null}
      onCancel={onClose}
      destroyOnClose
    >
      <div className="settings-toolbar">
        <span style={{ color: 'var(--text-2)' }}>共 {items.length} 个已删除会话</span>
        <Popconfirm
          title="清空回收站"
          description="将永久删除所有已删除的会话、项目及历史数据，不可恢复。"
          okText="永久删除"
          cancelText="取消"
          okButtonProps={{ danger: true, loading: purging }}
          onConfirm={() => void onPurge()}
        >
          <Button danger icon={<DeleteOutlined />} disabled={items.length === 0}>
            清空回收站
          </Button>
        </Popconfirm>
      </div>
      {items.length === 0 && !loading ? (
        <Empty text="回收站为空" />
      ) : (
        <div className="recycle-list">
          {items.map((s) => (
            <div className="recycle-row" key={s.id}>
              <div className="rc-main">
                <div className="rc-title" title={s.title}>
                  {s.title}
                </div>
                <div className="rc-meta">
                  {s.projectName || `项目 #${s.projectId}`} · 删除于{' '}
                  {s.deletedAt ?? s.updatedAt}
                </div>
              </div>
              <Button
                icon={<UndoOutlined />}
                loading={restoring === s.id}
                onClick={() => void onRestore(s)}
              >
                恢复
              </Button>
            </div>
          ))}
        </div>
      )}
    </Modal>
  );
}
