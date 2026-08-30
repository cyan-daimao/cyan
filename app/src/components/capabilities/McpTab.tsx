import { useState } from 'react';
import { Alert, Button, Segmented, Table, Tag, Tooltip } from 'antd';
import { PlusOutlined } from '@ant-design/icons';
import type { ColumnsType } from 'antd/es/table';
import type { McpServerDTO } from '../../types';
import { useConfigStore } from '../../stores/configStore';
import { confirmDanger, toast } from '../../utils/feedback';
import { Empty } from '../common/Empty';
import { McpFormModal } from './McpFormModal';
import { McpMarketView } from './McpMarketView';

/** 设置 - MCP 服务器：启用/禁用/编辑/删除 + 连接状态展示 */
export function McpTab() {
  const servers = useConfigStore((s) => s.mcpServers);
  const loading = useConfigStore((s) => s.loadingMcp);
  const toggleMcp = useConfigStore((s) => s.toggleMcp);
  const deleteMcp = useConfigStore((s) => s.deleteMcp);

  const [formOpen, setFormOpen] = useState(false);
  const [editing, setEditing] = useState<McpServerDTO | null>(null);
  const [toggling, setToggling] = useState<number | null>(null);
  /** 已安装 / 市场 视图切换 */
  const [view, setView] = useState<'installed' | 'market'>('installed');

  const onToggle = async (s: McpServerDTO) => {
    const enable = s.status === 'disabled';
    setToggling(s.id);
    const ok = await toggleMcp(s.id, enable);
    setToggling(null);
    if (ok) {
      if (enable) toast.success(`已启用并连接 ${s.name}`);
      else toast.info(`已禁用 ${s.name}`);
    }
  };

  const columns: ColumnsType<McpServerDTO> = [
    {
      title: '名称',
      dataIndex: 'name',
      render: (v: string) => <b>{v}</b>,
    },
    {
      title: '命令 / 服务地址',
      dataIndex: 'command',
      ellipsis: true,
      render: (v: string, s) => (
        <span className="mono" title={v}>
          {s.transport === 'sse' ? <Tag color="blue">SSE</Tag> : null}
          {v}
        </span>
      ),
    },
    {
      title: '状态',
      dataIndex: 'status',
      width: 110,
      render: (v: McpServerDTO['status'], s) => {
        if (v === 'connected') return <Tag color="success">已连接</Tag>;
        if (v === 'error') {
          return (
            <Tooltip title={s.lastError ?? '连接失败'}>
              <Tag color="error">连接失败</Tag>
            </Tooltip>
          );
        }
        return <Tag>已禁用</Tag>;
      },
    },
    {
      title: '工具数',
      dataIndex: 'tools',
      width: 80,
      render: (v: number, s) => (s.status === 'connected' ? v : '—'),
    },
    {
      title: '操作',
      key: 'ops',
      width: 190,
      render: (_, s) => (
        <span>
          <Button type="link" size="small" loading={toggling === s.id} onClick={() => void onToggle(s)}>
            {s.status === 'disabled' ? '启用' : '禁用'}
          </Button>
          <Button
            type="link"
            size="small"
            onClick={() => {
              setEditing(s);
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
                title: '删除 MCP 服务器',
                content: (
                  <span>
                    确定删除服务器 <b>{s.name}</b> 吗？正在使用的会话将失去其工具。
                  </span>
                ),
                okText: '删除',
                onOk: async () => {
                  if (await deleteMcp(s.id)) toast.success('服务器已删除');
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
      <Segmented
        value={view}
        onChange={(v) => setView(v as 'installed' | 'market')}
        options={[
          { label: '已安装', value: 'installed' },
          { label: '市场', value: 'market' },
        ]}
        style={{ marginBottom: 12 }}
      />
      {view === 'market' ? (
        <McpMarketView />
      ) : (
        <>
      <Alert
        type="warning"
        showIcon
        style={{ marginBottom: 12 }}
        message="MCP 服务器以子进程方式运行，请只添加可信来源；连接失败的服务器会标记为错误并自动跳过。"
      />
      <div className="settings-toolbar">
        <span style={{ color: 'var(--text-2)' }}>已配置 {servers.length} 个服务器</span>
        <Button
          type="primary"
          icon={<PlusOutlined />}
          onClick={() => {
            setEditing(null);
            setFormOpen(true);
          }}
        >
          新增服务器
        </Button>
      </div>
      <Table<McpServerDTO>
        rowKey="id"
        columns={columns}
        dataSource={servers}
        loading={loading}
        locale={{ emptyText: <Empty text="还没有配置 MCP 服务器" /> }}
        pagination={false}
      />
      <McpFormModal open={formOpen} editing={editing} onClose={() => setFormOpen(false)} />
        </>
      )}
    </div>
  );
}
