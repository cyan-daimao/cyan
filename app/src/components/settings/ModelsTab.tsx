import { useMemo, useState } from 'react';
import { Button, Input, Table, Tag } from 'antd';
import { PlusOutlined } from '@ant-design/icons';
import type { ColumnsType } from 'antd/es/table';
import type { ModelDTO } from '../../types';
import { useConfigStore } from '../../stores/configStore';
import { confirmDanger, toast } from '../../utils/feedback';
import { fmtCtxWindow } from '../../utils/format';
import { Empty } from '../common/Empty';
import { ModelFormModal } from './ModelFormModal';

/** 设置 - 模型配置：搜索 / 分页 / 设为默认 / 删除保护默认行 */
export function ModelsTab() {
  const models = useConfigStore((s) => s.models);
  const loading = useConfigStore((s) => s.loadingModels);
  const setDefault = useConfigStore((s) => s.setDefault);
  const deleteModel = useConfigStore((s) => s.deleteModel);

  const [kw, setKw] = useState('');
  const [page, setPage] = useState(1);
  const [formOpen, setFormOpen] = useState(false);
  const [editing, setEditing] = useState<ModelDTO | null>(null);

  const filtered = useMemo(() => {
    const k = kw.trim().toLowerCase();
    if (!k) return models;
    return models.filter(
      (m) => m.name.toLowerCase().includes(k) || m.provider.toLowerCase().includes(k),
    );
  }, [models, kw]);

  const columns: ColumnsType<ModelDTO> = [
    {
      title: '模型',
      dataIndex: 'name',
      render: (v: string) => <span className="mono">{v}</span>,
    },
    { title: 'Provider', dataIndex: 'provider' },
    {
      title: '上下文',
      dataIndex: 'contextWindow',
      width: 90,
      render: (v: number) => fmtCtxWindow(v),
    },
    {
      title: '状态',
      dataIndex: 'status',
      width: 90,
      render: (v: ModelDTO['status']) =>
        v === 'enabled' ? <Tag color="success">启用</Tag> : <Tag>停用</Tag>,
    },
    {
      title: '默认',
      dataIndex: 'isDefault',
      width: 80,
      render: (v: boolean) => (v ? <Tag color="processing">默认</Tag> : null),
    },
    {
      title: '操作',
      key: 'ops',
      width: 210,
      render: (_, m) => (
        <span>
          <Button
            type="link"
            size="small"
            onClick={() => {
              setEditing(m);
              setFormOpen(true);
            }}
          >
            编辑
          </Button>
          <Button
            type="link"
            size="small"
            disabled={m.isDefault || m.status === 'disabled'}
            onClick={() => {
              void setDefault(m.id).then((ok) => {
                if (ok) toast.success('默认模型已更新');
              });
            }}
          >
            设为默认
          </Button>
          <Button
            type="link"
            size="small"
            danger
            disabled={m.isDefault}
            onClick={() =>
              confirmDanger({
                title: '删除模型',
                content: (
                  <span>
                    确定删除模型 <b className="mono">{m.name}</b> 的配置吗？
                  </span>
                ),
                okText: '删除',
                onOk: async () => {
                  if (await deleteModel(m.id)) toast.success('模型配置已删除');
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
      <div className="settings-toolbar">
        <Input
          style={{ width: 220 }}
          placeholder="搜索名称 / Provider…"
          value={kw}
          allowClear
          autoCapitalize="off"
          autoCorrect="off"
          spellCheck={false}
          onChange={(e) => {
            setKw(e.target.value);
            setPage(1);
          }}
        />
        <Button
          type="primary"
          icon={<PlusOutlined />}
          onClick={() => {
            setEditing(null);
            setFormOpen(true);
          }}
        >
          新增模型
        </Button>
      </div>
      <Table<ModelDTO>
        rowKey="id"
        columns={columns}
        dataSource={filtered}
        loading={loading}
        locale={{ emptyText: <Empty text="没有匹配的模型配置" /> }}
        pagination={{
          pageSize: 5,
          current: page,
          onChange: setPage,
          showTotal: (t) => `共 ${t} 条`,
          hideOnSinglePage: false,
        }}
      />
      <ModelFormModal open={formOpen} editing={editing} onClose={() => setFormOpen(false)} />
    </div>
  );
}
