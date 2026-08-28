import { useEffect, useState } from 'react';
import { Alert, Button, Table, Tag } from 'antd';
import { PlusOutlined } from '@ant-design/icons';
import type { ColumnsType } from 'antd/es/table';
import type { PermAction, PermRuleDTO } from '../../types';
import { useConfigStore } from '../../stores/configStore';
import { confirmDanger, toast } from '../../utils/feedback';
import { Empty } from '../common/Empty';
import { PermRuleFormModal } from '../perms/PermRuleFormModal';

const ACTION_TAG: Record<PermAction, { color: string; text: string }> = {
  allow: { color: 'success', text: 'allow 放行' },
  ask: { color: 'warning', text: 'ask 询问' },
  deny: { color: 'error', text: 'deny 拒绝' },
};

/** 设置 - 权限规则：仅管理全局规则（对所有项目/会话生效） */
export function PermsTab() {
  const rules = useConfigStore((s) => s.globalRules);
  const loadGlobalRules = useConfigStore((s) => s.loadGlobalRules);
  const deletePermRule = useConfigStore((s) => s.deletePermRule);

  const [formOpen, setFormOpen] = useState(false);
  const [editing, setEditing] = useState<PermRuleDTO | null>(null);

  useEffect(() => {
    void loadGlobalRules();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

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
                title: '删除全局规则',
                content: (
                  <span>
                    删除全局规则{' '}
                    <b className="mono">
                      {r.tool} {r.pattern}
                    </b>{' '}
                    后，所有项目匹配的操作将回退到「询问」。
                  </span>
                ),
                okText: '删除',
                onOk: async () => {
                  if (await deletePermRule(r.id)) {
                    toast.success('规则已删除');
                    void loadGlobalRules();
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
    <div>
      <Alert
        type="info"
        showIcon
        style={{ marginBottom: 12 }}
        message="这里维护的是全局规则，对所有项目的所有会话生效。项目级/会话级规则请在对话输入区的「规则」入口维护。"
      />
      <div className="settings-toolbar">
        <span style={{ color: 'var(--text-2)' }}>共 {rules.length} 条全局规则</span>
        <Button
          type="primary"
          icon={<PlusOutlined />}
          onClick={() => {
            setEditing(null);
            setFormOpen(true);
          }}
        >
          新增全局规则
        </Button>
      </div>
      <Table<PermRuleDTO>
        rowKey="id"
        columns={columns}
        dataSource={rules}
        locale={{ emptyText: <Empty text="没有全局规则" /> }}
        pagination={false}
      />
      <PermRuleFormModal
        open={formOpen}
        editing={editing}
        allowScopes={['global']}
        onClose={() => setFormOpen(false)}
        onSaved={() => void loadGlobalRules()}
      />
    </div>
  );
}
