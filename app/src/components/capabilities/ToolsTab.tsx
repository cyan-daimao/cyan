import { Alert, Switch, Table, Tag } from 'antd';
import type { ColumnsType } from 'antd/es/table';
import { useConfigStore } from '../../stores/configStore';

/** 内置工具清单（与后端 runner.rs builtin_tools 一致） */
const BUILTIN_TOOLS: { name: string; desc: string; write: boolean }[] = [
  { name: 'Read', desc: '读取项目内文本文件内容', write: false },
  { name: 'Grep', desc: '按正则在项目内搜索文件内容（路径:行号输出）', write: false },
  { name: 'Glob', desc: '按 glob 模式匹配项目内文件路径', write: false },
  { name: 'WebFetch', desc: '抓取公开网页内容（网络访问，30s 超时）', write: false },
  { name: 'Write', desc: '写入（新建或覆盖）项目内文件', write: true },
  { name: 'Edit', desc: '对项目内文件做唯一字符串替换', write: true },
  { name: 'MultiEdit', desc: '同一文件多处替换，任一失败整次不写盘', write: true },
  { name: 'Bash', desc: '在项目根目录执行 Bash 命令', write: true },
  { name: 'TodoWrite', desc: '更新任务 TODO 列表', write: false },
];

interface ToolRow {
  name: string;
  desc: string;
  write: boolean;
}

/** 能力 - 工具：内置工具启用/禁用（禁用后不下发给 LLM，随每次任务生效） */
export function ToolsTab() {
  const disabledTools = useConfigStore((s) => s.disabledTools);
  const setToolEnabled = useConfigStore((s) => s.setToolEnabled);

  const columns: ColumnsType<ToolRow> = [
    {
      title: '工具',
      dataIndex: 'name',
      width: 130,
      render: (v: string) => <Tag className="mono">{v}</Tag>,
    },
    { title: '说明', dataIndex: 'desc' },
    {
      title: '类型',
      dataIndex: 'write',
      width: 90,
      render: (v: boolean) =>
        v ? <Tag color="warning">写操作</Tag> : <Tag color="success">只读</Tag>,
    },
    {
      title: '启用',
      key: 'enabled',
      width: 80,
      render: (_, r) => (
        <Switch
          size="small"
          checked={!disabledTools.includes(r.name)}
          onChange={(checked) => setToolEnabled(r.name, checked)}
        />
      ),
    },
  ];

  return (
    <div>
      <Alert
        type="info"
        showIcon
        style={{ marginBottom: 12 }}
        message="禁用的工具不会下发给模型，从下一次任务开始生效。写操作类工具仍受权限模式与权限规则约束。"
      />
      <Table<ToolRow>
        rowKey="name"
        columns={columns}
        dataSource={BUILTIN_TOOLS}
        pagination={false}
      />
    </div>
  );
}
