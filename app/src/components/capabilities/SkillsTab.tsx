import { useEffect, useState } from 'react';
import { Alert, Button, Segmented, Switch, Table, Tag, Tooltip } from 'antd';
import type { ColumnsType } from 'antd/es/table';
import { PlusOutlined } from '@ant-design/icons';
import type { SkillDTO } from '../../types';
import { useSkillStore } from '../../stores/skillStore';
import { useProjectStore } from '../../stores/projectStore';
import { confirmDanger, toast } from '../../utils/feedback';
import { Empty } from '../common/Empty';
import { SkillFormModal } from './SkillFormModal';
import { SkillMarketView } from './MarketplaceView';

/** 能力面板 - 技能 Tab：列表（名称/描述/来源/启用）+ 新增/编辑/删除 */
export function SkillsTab() {
  const skills = useSkillStore((s) => s.skills);
  const loading = useSkillStore((s) => s.loading);
  const load = useSkillStore((s) => s.load);
  const remove = useSkillStore((s) => s.remove);
  const toggle = useSkillStore((s) => s.toggle);
  const project = useProjectStore((s) => s.current);

  const [formOpen, setFormOpen] = useState(false);
  const [editing, setEditing] = useState<SkillDTO | null>(null);
  const [toggling, setToggling] = useState<string | null>(null);
  /** 已安装 / 市场 视图切换 */
  const [view, setView] = useState<'installed' | 'market'>('installed');

  // 面板打开时按当前项目加载（无项目只列全局）
  useEffect(() => {
    void load(project?.path ?? '', true);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [project?.path]);

  const onToggle = async (s: SkillDTO, checked: boolean) => {
    const key = `${s.source}:${s.id}`;
    setToggling(key);
    const ok = await toggle(s, project?.path ?? '');
    setToggling(null);
    if (ok) toast.success(`已${checked ? '启用' : '停用'} ${s.name}`);
  };

  const columns: ColumnsType<SkillDTO> = [
    {
      title: '名称',
      dataIndex: 'name',
      render: (v: string, s) => (
        <span>
          <b>{v}</b> <span className="mono" style={{ color: 'var(--text-3)', fontSize: 12 }}>/{s.id}</span>
        </span>
      ),
    },
    {
      title: '描述',
      dataIndex: 'description',
      ellipsis: true,
      render: (v: string) => <span title={v}>{v}</span>,
    },
    {
      title: '来源',
      dataIndex: 'source',
      width: 150,
      render: (v: SkillDTO['source'], s) => {
        const origin =
          v === 'plugin' ? (
            <Tag color="purple">插件·{s.pluginName}</Tag>
          ) : v === 'global' ? (
            <Tag>全局</Tag>
          ) : (
            <Tag color="processing">项目</Tag>
          );
        // 市场安装的技能在来源旁加小标记，tooltip 展示来源仓库
        return (
          <span style={{ display: 'inline-flex', gap: 4 }}>
            {origin}
            {s.marketRepo ? (
              <Tooltip title={`来自技能市场仓库 ${s.marketRepo}`}>
                <Tag color="cyan">市场</Tag>
              </Tooltip>
            ) : null}
          </span>
        );
      },
    },
    {
      title: '启用',
      dataIndex: 'enabled',
      width: 80,
      render: (v: boolean, s) => (
        <Switch
          size="small"
          checked={v}
          disabled={s.source === 'plugin'}
          loading={toggling === `${s.source}:${s.id}`}
          onChange={(checked) => void onToggle(s, checked)}
        />
      ),
    },
    {
      title: '操作',
      key: 'ops',
      width: 130,
      render: (_, s) =>
        s.source === 'plugin' ? (
          <span style={{ color: 'var(--text-3)', fontSize: 12 }}>随插件管理</span>
        ) : (
        <span>
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
                title: '删除技能',
                content: (
                  <span>
                    确定删除技能 <b>{s.name}</b>（<span className="mono">/{s.id}</span>）吗？
                    对应 Markdown 文件将被移除。
                  </span>
                ),
                okText: '删除',
                onOk: async () => {
                  const ok = await remove(
                    s.source as 'global' | 'project',
                    s.id,
                    s.source === 'project' ? project?.path : undefined,
                  );
                  if (ok) toast.success('技能已删除');
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
        <SkillMarketView />
      ) : (
        <>
      <Alert
        type="info"
        showIcon
        style={{ marginBottom: 12 }}
        message="技能 = Markdown 定义的工作流模板。输入框以 / 开头触发补全，选中后正文展开；$ARGUMENTS 由用户继续填写。同名技能项目级覆盖全局。"
      />
      <div className="settings-toolbar">
        <span style={{ color: 'var(--text-2)' }}>共 {skills.length} 个技能</span>
        <Button
          type="primary"
          icon={<PlusOutlined />}
          onClick={() => {
            setEditing(null);
            setFormOpen(true);
          }}
        >
          新增技能
        </Button>
      </div>
      <Table<SkillDTO>
        rowKey={(s) => `${s.source}:${s.id}`}
        columns={columns}
        dataSource={skills}
        loading={loading}
        locale={{ emptyText: <Empty text="还没有技能，点击「新增技能」创建第一个" /> }}
        pagination={false}
      />
      <SkillFormModal open={formOpen} editing={editing} onClose={() => setFormOpen(false)} />
        </>
      )}
    </div>
  );
}
