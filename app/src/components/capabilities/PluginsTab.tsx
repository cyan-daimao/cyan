import { useEffect, useState } from 'react';
import { Alert, Button, Segmented, Switch, Table, Tag } from 'antd';
import type { ColumnsType } from 'antd/es/table';
import { FileZipOutlined, FolderOpenOutlined } from '@ant-design/icons';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import type { PluginDTO } from '../../types';
import { usePluginStore } from '../../stores/pluginStore';
import { confirmDanger, toast } from '../../utils/feedback';
import { Empty } from '../common/Empty';
import { PluginMarketView } from './MarketplaceView';

/** 能力面板 - 插件 Tab：声明式能力包的安装 / 启停 / 卸载（PLUGIN_DESIGN 第 3 节） */
export function PluginsTab() {
  const plugins = usePluginStore((s) => s.plugins);
  const loading = usePluginStore((s) => s.loading);
  const load = usePluginStore((s) => s.load);
  const install = usePluginStore((s) => s.install);
  const toggle = usePluginStore((s) => s.toggle);
  const remove = usePluginStore((s) => s.remove);

  /** 安装中来源：zip / dir */
  const [installing, setInstalling] = useState<'zip' | 'dir' | null>(null);
  const [toggling, setToggling] = useState<number | null>(null);
  /** 已安装 / 插件市场 视图切换 */
  const [view, setView] = useState<'installed' | 'market'>('installed');

  // 面板打开时加载一次
  useEffect(() => {
    void load(true);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  /** 选择来源并安装：zip 文件过滤 / 目录选择 */
  const onInstall = async (kind: 'zip' | 'dir') => {
    const selected = await openDialog(
      kind === 'zip'
        ? { filters: [{ name: 'ZIP', extensions: ['zip'] }] }
        : { directory: true },
    );
    if (typeof selected !== 'string' || !selected) return; // 用户取消
    setInstalling(kind);
    const dto = await install(selected);
    setInstalling(null);
    if (dto) {
      toast.success(
        `已安装 ${dto.name} v${dto.version}（技能 ${dto.skillCount} · MCP ${dto.mcpCount} · 规则 ${dto.ruleCount}）`,
      );
    }
  };

  const onToggle = async (p: PluginDTO, enable: boolean) => {
    setToggling(p.id);
    const ok = await toggle(p.id, enable);
    setToggling(null);
    if (ok) toast.success(`已${enable ? '启用' : '禁用'} ${p.name}`);
  };

  const columns: ColumnsType<PluginDTO> = [
    {
      title: '名称',
      dataIndex: 'name',
      render: (v: string, p) => (
        <span>
          <b>{v}</b>{' '}
          <span className="mono" style={{ color: 'var(--text-3)', fontSize: 12 }}>
            v{p.version}
          </span>
        </span>
      ),
    },
    { title: '作者', dataIndex: 'author', width: 110, ellipsis: true },
    {
      title: '描述',
      dataIndex: 'description',
      ellipsis: true,
      render: (v: string) => <span title={v}>{v}</span>,
    },
    {
      title: '内容物',
      key: 'contents',
      width: 170,
      render: (_, p) => (
        <span style={{ color: 'var(--text-2)', fontSize: 13 }}>
          技能 {p.skillCount} · MCP {p.mcpCount} · 规则 {p.ruleCount}
        </span>
      ),
    },
    {
      title: '状态',
      dataIndex: 'status',
      width: 90,
      render: (v: PluginDTO['status']) =>
        v === 'enabled' ? <Tag color="success">已启用</Tag> : <Tag>已禁用</Tag>,
    },
    {
      title: '启用',
      key: 'switch',
      width: 80,
      render: (_, p) => (
        <Switch
          size="small"
          checked={p.status === 'enabled'}
          loading={toggling === p.id}
          onChange={(checked) => void onToggle(p, checked)}
        />
      ),
    },
    {
      title: '操作',
      key: 'ops',
      width: 90,
      render: (_, p) => (
        <Button
          type="link"
          size="small"
          danger
          onClick={() =>
            confirmDanger({
              title: '卸载插件',
              content: (
                <span>
                  确定卸载插件 <b>{p.name}</b> 吗？将连带摘除其携带的技能（{p.skillCount}）、MCP
                  服务器（{p.mcpCount}）与权限规则（{p.ruleCount}）。
                </span>
              ),
              okText: '卸载',
              onOk: async () => {
                if (await remove(p.id)) toast.success(`已卸载 ${p.name}`);
              },
            })
          }
        >
          卸载
        </Button>
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
          { label: '插件市场', value: 'market' },
        ]}
        style={{ marginBottom: 12 }}
      />
      {view === 'market' ? (
        <PluginMarketView />
      ) : (
        <>
          <Alert
            type="warning"
            showIcon
            style={{ marginBottom: 12 }}
            message="插件是声明式能力包（技能 / MCP 服务器 / 权限规则预设）。请只安装可信来源的插件；禁用后其内容物整体摘除。"
          />
      <div className="settings-toolbar">
        <span style={{ color: 'var(--text-2)' }}>已安装 {plugins.length} 个插件</span>
        <span style={{ display: 'inline-flex', gap: 8 }}>
          <Button
            icon={<FileZipOutlined />}
            loading={installing === 'zip'}
            disabled={installing !== null}
            onClick={() => void onInstall('zip')}
          >
            从 ZIP 安装
          </Button>
          <Button
            icon={<FolderOpenOutlined />}
            loading={installing === 'dir'}
            disabled={installing !== null}
            onClick={() => void onInstall('dir')}
          >
            从文件夹安装
          </Button>
        </span>
      </div>
      <Table<PluginDTO>
        rowKey="id"
        columns={columns}
        dataSource={plugins}
        loading={loading}
        locale={{ emptyText: <Empty text="还没有安装插件" /> }}
        pagination={false}
      />
        </>
      )}
    </div>
  );
}
