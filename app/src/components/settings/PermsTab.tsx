import { useState } from 'react';
import { Alert, Button, Table, Tag } from 'antd';
import type { ColumnsType } from 'antd/es/table';
import type { PermAction, PermRuleDTO } from '../../types';
import { useConfigStore } from '../../stores/configStore';
import { confirmDanger, toast } from '../../utils/feedback';
import { Empty } from '../common/Empty';
import { PermRuleFormModal } from './PermRuleFormModal';

const ACTION_TAG: Record<PermAction, { color: string; text: string }> = {
  allow: { color: 'success', text: 'allow 放行' },
  ask: { color: 'warning', text: 'ask 询问' },
  deny: { color: 'error', text: 'deny 拒绝' },
};

/** 设置 - 权限规则：增改删 + 命中顺序说明 */
export function PermsTab() {
  const rules = useConfigStore((s) => s.permRules);
  const loading = useConfigStore((s) => s.loadingPerms);
  const deletePermRule = useConfigStore((s) => s.deletePermRule);

  const [formOpen, setFormOpen] = useState(false);
  const [editing, setEditing] = useState<PermRuleDTO | null>(null);

  const columns: ColumnsType<PermRuleDTO> = [
    { title: '#', key: 'idx', width: 50, render: (_, __, i) => i + 1 },
    {
      title: '工具',
      dataIndex: 'tool',
      width: 110,
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
      width: 120,
      render: (v: PermAction) => <Tag color={ACTION_TAG[v].color}>{ACTION_TAG[v].text}</Tag>,
    },
    {
      title: '操作',
      key: 'ops',
      width: 130,
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
                  if (await deletePermRule(r.id)) toast.success('规则已删除');
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
    <div>
      <Alert
        type="info"
        showIcon
        style={{ marginBottom: 12 }}
        message="规则自上而下匹配，命中即生效。deny 永远优先；ask 弹出审批卡片；allow 静默放行。"
      />
      <div className="settings-toolbar">
        <span style={{ color: 'var(--text-2)' }}>共 {rules.length} 条规则</span>
        <Button
          type="primary"
          onClick={() => {
            setEditing(null);
            setFormOpen(true);
          }}
        >
          ＋ 新增规则
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
      <PermRuleFormModal open={formOpen} editing={editing} onClose={() => setFormOpen(false)} />
    </div>
  );
}
