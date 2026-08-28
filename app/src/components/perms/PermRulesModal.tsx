import { useEffect, useState } from 'react';
import { Alert, Button, Modal, Table, Tag } from 'antd';
import { PlusOutlined } from '@ant-design/icons';
import type { ColumnsType } from 'antd/es/table';
import type { PermAction, PermRuleDTO, RuleScope } from '../../types';
import { useConfigStore } from '../../stores/configStore';
import { useSessionStore } from '../../stores/sessionStore';
import { useProjectStore } from '../../stores/projectStore';
import { confirmDanger, toast } from '../../utils/feedback';
import { Empty } from '../common/Empty';
import { PermRuleFormModal } from './PermRuleFormModal';

const ACTION_TAG: Record<PermAction, { color: string; text: string }> = {
  allow: { color: 'success', text: 'allow 放行' },
  ask: { color: 'warning', text: 'ask 询问' },
  deny: { color: 'error', text: 'deny 拒绝' },
};

const SCOPE_TAG: Record<RuleScope, { color?: string; text: string }> = {
  global: { text: '全局' },
  project: { color: 'cyan', text: '本项目' },
  session: { color: 'processing', text: '本会话' },
};

interface PermRulesModalProps {
  open: boolean;
  onClose: () => void;
}

/** 会话级权限规则弹窗：展示当前会话可见的全部规则；新增只能选「本项目 / 本会话」 */
export function PermRulesModal({ open, onClose }: PermRulesModalProps) {
  const activeId = useSessionStore((s) => s.activeId);
  const project = useProjectStore((s) => s.current);
  const rules = useConfigStore((s) => s.permRules);
  const loading = useConfigStore((s) => s.loadingPerms);
  const loadVisibleRules = useConfigStore((s) => s.loadVisibleRules);
  const deletePermRule = useConfigStore((s) => s.deletePermRule);

  const [formOpen, setFormOpen] = useState(false);
  const [editing, setEditing] = useState<PermRuleDTO | null>(null);

  const reload = () => {
    if (activeId !== null && project) void loadVisibleRules(activeId, project.id);
  };

  // 打开弹窗 / 切换会话时按会话加载可见规则
  useEffect(() => {
    if (open) reload();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, activeId, project?.id]);

  const columns: ColumnsType<PermRuleDTO> = [
    { title: '#', key: 'idx', width: 44, render: (_, __, i) => i + 1 },
    {
      title: '范围',
      key: 'scope',
      width: 86,
      render: (_, r) => <Tag color={SCOPE_TAG[r.scope].color}>{SCOPE_TAG[r.scope].text}</Tag>,
    },
    {
      title: '工具',
      dataIndex: 'tool',
      width: 100,
      render: (v: string) => <Tag className="mono">{v}</Tag>,
    },
    {
      title: '匹配模式',
      dataIndex: 'pattern',
      render: (v: string) => <span className="mono">{v}</span>,
    },
    {
      title: '动作',
      dataIndex: 'action',
      width: 110,
      render: (v: PermAction) => <Tag color={ACTION_TAG[v].color}>{ACTION_TAG[v].text}</Tag>,
    },
    {
      title: '操作',
      key: 'ops',
      width: 120,
      render: (_, r) => (
        <span>
          <Button
            type="link"
            size="small"
            onClick={() => {
              setEditing(r);
              setFormOpen(true);
            }}
          >
            编辑
          </Button>
          <Button
            type="link"
            size="small"
            danger
            onClick={() =>
              confirmDanger({
                title: '删除权限规则',
                content: (
                  <span>
                    删除规则{' '}
                    <b className="mono">
                      {r.tool} {r.pattern}
                    </b>{' '}
                    后，匹配的操作将回退到「询问」。
                  </span>
                ),
                okText: '删除',
                onOk: async () => {
                  if (await deletePermRule(r.id)) {
                    toast.success('规则已删除');
                    reload();
                  }
                },
              })
            }
          >
            删除
          </Button>
        </span>
      ),
    },
  ];

  return (
    <Modal
      open={open}
      title="权限规则（当前会话）"
      width={780}
      footer={null}
      onCancel={onClose}
      destroyOnClose
    >
      <Alert
        type="info"
        showIcon
        style={{ marginBottom: 12 }}
        message="规则自上而下匹配，命中即生效。这里新增的规则只能作用于「本项目 / 本会话」；全局规则请到「设置 - 权限规则」维护。deny 永远优先；ask 弹出审批卡片；allow 静默放行。"
      />
      <div className="settings-toolbar">
        <span style={{ color: 'var(--text-2)' }}>共 {rules.length} 条规则</span>
        <Button
          type="primary"
          icon={<PlusOutlined />}
          disabled={activeId === null || !project}
          onClick={() => {
            setEditing(null);
            setFormOpen(true);
          }}
        >
          新增规则
        </Button>
      </div>
      <Table<PermRuleDTO>
        rowKey="id"
        columns={columns}
        dataSource={rules}
        loading={loading}
        locale={{ emptyText: <Empty text="没有权限规则，所有危险操作都会询问" /> }}
        pagination={false}
      />
      <PermRuleFormModal
        open={formOpen}
        editing={editing}
        allowScopes={['project', 'session']}
        onClose={() => setFormOpen(false)}
        onSaved={reload}
      />
    </Modal>
  );
}
