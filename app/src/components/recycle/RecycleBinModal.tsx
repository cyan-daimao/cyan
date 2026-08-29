import { useCallback, useEffect, useState } from 'react';
import type { ReactNode } from 'react';
import { Button, Modal, Popconfirm, Tabs } from 'antd';
import { DeleteOutlined, UndoOutlined } from '@ant-design/icons';
import type { RecycleBinDTO, RecycleKind } from '../../types';
import { listRecycleBin, purgeRecycleBin, restoreRecycleItem } from '../../services/session';
import { useSessionStore } from '../../stores/sessionStore';
import { useProjectStore } from '../../stores/projectStore';
import { errText, toast } from '../../utils/feedback';
import { Empty } from '../common/Empty';

interface RecycleBinModalProps {
  open: boolean;
  onClose: () => void;
}

/** 回收站行视图模型 */
interface RecycleRow {
  key: string;
  kind: RecycleKind;
  id: number | string;
  title: ReactNode;
  meta: string;
}

const EMPTY_BIN: RecycleBinDTO = {
  sessions: [],
  projects: [],
  models: [],
  mcpServers: [],
  plugins: [],
  permRules: [],
  skills: [],
};

/** 回收站：全对象（会话/项目/模型/MCP/插件/规则/技能）Tabs 分组恢复与清空 */
export function RecycleBinModal({ open, onClose }: RecycleBinModalProps) {
  const [bin, setBin] = useState<RecycleBinDTO>(EMPTY_BIN);
  const [loading, setLoading] = useState(false);
  /** 恢复中的行：`${kind}:${id}` */
  const [restoring, setRestoring] = useState<string | null>(null);
  const [purging, setPurging] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setBin(await listRecycleBin());
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

  /** 从对应 Tab 列表移除已恢复的行 */
  const removeRow = (kind: RecycleKind, id: number | string) => {
    setBin((prev) => {
      const next = { ...prev };
      switch (kind) {
        case 'session':
          next.sessions = prev.sessions.filter((x) => x.id !== id);
          break;
        case 'project':
          next.projects = prev.projects.filter((x) => x.id !== id);
          break;
        case 'model':
          next.models = prev.models.filter((x) => x.id !== id);
          break;
        case 'mcp':
          next.mcpServers = prev.mcpServers.filter((x) => x.id !== id);
          break;
        case 'plugin':
          next.plugins = prev.plugins.filter((x) => x.id !== id);
          break;
        case 'permRule':
          next.permRules = prev.permRules.filter((x) => x.id !== id);
          break;
        case 'skill':
          next.skills = (prev.skills ?? []).filter((x) => x.id !== id);
          break;
      }
      return next;
    });
  };

  const onRestore = async (row: RecycleRow) => {
    const key = row.key;
    setRestoring(key);
    try {
      await restoreRecycleItem(row.kind, row.id);
      removeRow(row.kind, row.id);
      // 恢复项目会自动带回随删会话；恢复会话在项目已删时连带恢复项目 → 都刷新侧栏
      if (row.kind === 'session' || row.kind === 'project') refreshSidebar();
      toast.success('已恢复');
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

  const renderList = (rows: RecycleRow[], emptyText: string) =>
    rows.length === 0 ? (
      <Empty text={emptyText} />
    ) : (
      <div className="recycle-list">
        {rows.map((r) => (
          <div className="recycle-row" key={r.key}>
            <div className="rc-main">
              <div className="rc-title">{r.title}</div>
              {r.meta ? <div className="rc-meta">{r.meta}</div> : null}
            </div>
            <Button
              icon={<UndoOutlined />}
              loading={restoring === r.key}
              onClick={() => void onRestore(r)}
            >
              恢复
            </Button>
          </div>
        ))}
      </div>
    );

  const fmtTime = (t?: string | null) => (t ? `删除于 ${t}` : '');

  const tabs: { key: string; label: string; rows: RecycleRow[]; emptyText: string }[] = [
    {
      key: 'session',
      label: '会话',
      emptyText: '没有已删除的会话',
      rows: bin.sessions.map((s) => ({
        key: `session:${s.id}`,
        kind: 'session' as const,
        id: s.id,
        title: s.title,
        meta: [s.projectName || `项目 #${s.projectId}`, fmtTime(s.deletedAt ?? s.updatedAt)]
          .filter(Boolean)
          .join(' · '),
      })),
    },
    {
      key: 'project',
      label: '项目',
      emptyText: '没有已删除的项目',
      rows: bin.projects.map((p) => ({
        key: `project:${p.id}`,
        kind: 'project' as const,
        id: p.id,
        title: p.name,
        meta: [p.path, fmtTime(p.deletedAt)].filter(Boolean).join(' · '),
      })),
    },
    {
      key: 'model',
      label: '模型',
      emptyText: '没有已删除的模型',
      rows: bin.models.map((m) => ({
        key: `model:${m.id}`,
        kind: 'model' as const,
        id: m.id,
        title: <span className="mono">{m.name}</span>,
        meta: [m.provider, fmtTime(m.deletedAt)].filter(Boolean).join(' · '),
      })),
    },
    {
      key: 'mcp',
      label: 'MCP',
      emptyText: '没有已删除的 MCP 服务器',
      rows: bin.mcpServers.map((s) => ({
        key: `mcp:${s.id}`,
        kind: 'mcp' as const,
        id: s.id,
        title: s.name,
        meta: [s.command, fmtTime(s.deletedAt)].filter(Boolean).join(' · '),
      })),
    },
    {
      key: 'plugin',
      label: '插件',
      emptyText: '没有已删除的插件',
      rows: bin.plugins.map((p) => ({
        key: `plugin:${p.id}`,
        kind: 'plugin' as const,
        id: p.id,
        title: (
          <span>
            {p.name} <span className="mono" style={{ color: 'var(--text-3)', fontSize: 12 }}>v{p.version}</span>
          </span>
        ),
        meta: fmtTime(p.deletedAt),
      })),
    },
    {
      key: 'permRule',
      label: '规则',
      emptyText: '没有已删除的权限规则',
      rows: bin.permRules.map((r) => ({
        key: `permRule:${r.id}`,
        kind: 'permRule' as const,
        id: r.id,
        title: (
          <span className="mono">
            {r.tool} {r.pattern}
          </span>
        ),
        meta: [
          r.action,
          r.scope === 'global' ? '全局' : r.scope === 'project' ? '项目' : '会话',
          fmtTime(r.deletedAt),
        ]
          .filter(Boolean)
          .join(' · '),
      })),
    },
  ];

  // 技能 Tab：后端可能不提供，有数据才显示
  const skills = bin.skills ?? [];
  if (skills.length > 0) {
    tabs.push({
      key: 'skill',
      label: '技能',
      emptyText: '没有已删除的技能',
      rows: skills.map((s) => ({
        key: `skill:${s.id}`,
        kind: 'skill' as const,
        id: s.id,
        title: s.name,
        meta: [
          `/${s.fileName}`,
          s.scope === 'global' ? '全局' : '项目',
          fmtTime(s.deletedAt),
        ]
          .filter(Boolean)
          .join(' · '),
      })),
    });
  }

  const total =
    bin.sessions.length +
    bin.projects.length +
    bin.models.length +
    bin.mcpServers.length +
    bin.plugins.length +
    bin.permRules.length +
    skills.length;

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
        <span style={{ color: 'var(--text-2)' }}>共 {total} 条已删除记录</span>
        <Popconfirm
          title="清空回收站"
          description="将永久删除所有已删除的会话、项目及历史数据，不可恢复。"
          okText="永久删除"
          cancelText="取消"
          okButtonProps={{ danger: true, loading: purging }}
          onConfirm={() => void onPurge()}
        >
          <Button danger icon={<DeleteOutlined />} disabled={total === 0}>
            清空回收站
          </Button>
        </Popconfirm>
      </div>
      <Tabs
        items={tabs.map((t) => ({
          key: t.key,
          label: t.rows.length > 0 ? `${t.label}（${t.rows.length}）` : t.label,
          children: loading && bin === EMPTY_BIN ? null : renderList(t.rows, t.emptyText),
        }))}
      />
    </Modal>
  );
}
