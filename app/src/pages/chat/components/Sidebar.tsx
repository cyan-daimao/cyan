import { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { Dropdown, Input } from 'antd';
import {
  ApiOutlined,
  BarChartOutlined,
  CommentOutlined,
  DeleteOutlined,
  FolderOpenOutlined,
  FolderOutlined,
  LeftOutlined,
  PlusOutlined,
  RestOutlined,
  SearchOutlined,
  SettingOutlined,
  UserOutlined,
} from '@ant-design/icons';
import logoUrl from '../../../assets/logo.png';
import { RecycleBinModal } from '../../../components/recycle/RecycleBinModal';
import { SessionItem } from '../../../components/session/SessionItem';
import { listSessions, deleteSession } from '../../../services/session';
import { useSessionStore } from '../../../stores/sessionStore';
import { useProjectStore } from '../../../stores/projectStore';
import { useAgentStore } from '../../../stores/agentStore';
import { useConfigStore } from '../../../stores/configStore';
import { confirmDanger, toast } from '../../../utils/feedback';
import type { PermMode, SessionSummaryDTO } from '../../../types';

/** 侧栏底部权限模式提示文案 */
const PERM_DESC: Record<PermMode, string> = {
  plan: '权限模式：计划（只读，不改文件不跑命令）',
  ask: '权限模式：询问（危险操作需确认）',
  auto: '权限模式：自动（白名单内全部放行）',
};

interface SidebarProps {
  collapsed: boolean;
  onToggle: () => void;
  onOpenProject: () => void;
  onOpenSettings: (tab: 'models') => void;
  onOpenSkills: () => void;
  onSelectSession: (id: number) => void;
  onDeleteSession: (id: number) => void;
}

/**
 * 通高侧栏：品牌行 / 导航 / 会话搜索 / 项目→会话二级树 / 底部用户卡片。
 * 一级 = 项目（点击展开并切换），二级 = 该项目下的会话；
 * 右键项目：Token 用量报表 / 移除项目；会话行的运行指示沿用 SessionItem。
 */
export function Sidebar({
  collapsed,
  onToggle,
  onOpenProject,
  onOpenSettings,
  onOpenSkills,
  onSelectSession,
  onDeleteSession,
}: SidebarProps) {
  const navigate = useNavigate();
  const sessions = useSessionStore((s) => s.sessions);
  const activeId = useSessionStore((s) => s.activeId);
  const searchKw = useSessionStore((s) => s.searchKw);
  const setSearchKw = useSessionStore((s) => s.setSearchKw);
  const loadSessions = useSessionStore((s) => s.loadSessions);
  const sessionRuns = useAgentStore((s) => s.sessionRuns);
  const project = useProjectStore((s) => s.current);
  const recents = useProjectStore((s) => s.recents);
  const openProject = useProjectStore((s) => s.open);
  const removeProject = useProjectStore((s) => s.remove);
  const permMode = useConfigStore((s) => s.permMode);

  /** 展开状态（默认仅当前项目展开） */
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  /** 非当前项目的会话缓存（path → 列表），展开时懒加载 */
  const [otherSessions, setOtherSessions] = useState<Record<string, SessionSummaryDTO[]>>({});
  /** 回收站弹窗 */
  const [recycleOpen, setRecycleOpen] = useState(false);

  // 搜索防抖 300ms，走后端 list_sessions keyword（作用于当前项目）
  useEffect(() => {
    if (!project) return;
    const t = setTimeout(() => {
      void loadSessions(project.path, searchKw || undefined);
    }, 300);
    return () => clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [searchKw, project?.path]);

  /** 拉取非当前项目的会话列表（展开时 / 后台运行状态变化时刷新） */
  const fetchOther = (path: string) => {
    listSessions(path)
      .then((list) => setOtherSessions((m) => ({ ...m, [path]: list })))
      .catch(() => {
        /* 项目可能已移除/路径失效，保持旧数据 */
      });
  };

  // 任意会话运行状态变化（后台任务完成等）时，刷新已展开的其他项目列表
  useEffect(() => {
    for (const path of Object.keys(otherSessions)) {
      if (path !== project?.path && expanded[path]) fetchOther(path);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessionRuns]);

  const isExpanded = (path: string) => expanded[path] ?? path === project?.path;

  /** 一级菜单点击：展开/收起；非当前项目同时切换（加载其会话） */
  const onProjectClick = (path: string) => {
    const next = !isExpanded(path);
    setExpanded((m) => ({ ...m, [path]: next }));
    if (next && path !== project?.path) {
      if (!otherSessions[path]) fetchOther(path);
      void openProject(path);
    }
  };

  /** 二级菜单点击：选中会话；跨项目时先切换项目再打开会话 */
  const onTreeSelectSession = (path: string, id: number) => {
    if (path === project?.path) {
      onSelectSession(id);
      return;
    }
    void (async () => {
      const ok = await openProject(path);
      if (ok) onSelectSession(id);
    })();
  };

  /** 二级菜单删除：当前项目走上层回调；其他项目本地确认后删除并刷新缓存 */
  const onTreeDeleteSession = (path: string, id: number) => {
    if (path === project?.path) {
      onDeleteSession(id);
      return;
    }
    const flag = sessionRuns[id];
    if (flag === 'running' || flag === 'waiting_approval') {
      toast.warning('该会话任务运行中，请先停止再删除');
      return;
    }
    const title = otherSessions[path]?.find((s) => s.id === id)?.title ?? '';
    confirmDanger({
      title: '删除会话',
      content: (
        <span>
          确定删除会话 <b>{title}</b> 及其全部消息记录吗？此操作不可恢复。
        </span>
      ),
      okText: '删除',
      onOk: async () => {
        await deleteSession(id);
        setOtherSessions((m) => ({
          ...m,
          [path]: (m[path] ?? []).filter((s) => s.id !== id),
        }));
        toast.success('会话已删除');
      },
    });
  };

  /** 右键项目：移除（不删磁盘文件） */
  const onRemoveProject = (path: string, name: string) => {
    confirmDanger({
      title: '移除项目',
      content: (
        <span>
          确定从列表移除项目 <b>{name}</b> 吗？仅移除记录，磁盘文件与历史会话数据不受影响。
        </span>
      ),
      okText: '移除',
      onOk: async () => {
        const ok = await removeProject(path);
        if (ok) {
          setOtherSessions((m) => {
            const { [path]: _drop, ...rest } = m;
            return rest;
          });
        }
      },
    });
  };

  /** 右键项目：在该项目下新建对话（跨项目时先切换，并展开该项目） */
  const onNewChat = (path: string) => {
    void (async () => {
      if (path !== project?.path) {
        const ok = await openProject(path);
        if (!ok) return;
      }
      setExpanded((m) => ({ ...m, [path]: true }));
      const id = await useSessionStore.getState().createSession(path);
      if (id !== null) {
        useAgentStore.getState().resetForSession();
        navigate(`/chat?s=${id}`);
      }
    })();
  };

  return (
    <aside className={`sidebar${collapsed ? ' collapsed' : ''}`}>
      <div className="side-top">
        <div className="side-brand">
          <img className="brand-logo" src={logoUrl} alt="cyan" />
          <span>cyan</span>
          <button className="side-collapse" title="收起侧边栏" onClick={onToggle}>
            <LeftOutlined />
          </button>
        </div>
        <button className="nav-item" onClick={onOpenSkills}>
          <ApiOutlined /> 技能 · MCP
        </button>
        <button className="nav-item" onClick={() => onOpenSettings('models')}>
          <SettingOutlined /> 设置
        </button>
        <button className="nav-item" onClick={() => setRecycleOpen(true)}>
          <RestOutlined /> 回收站
        </button>
      </div>
      <div className="session-search">
        <Input
          prefix={<SearchOutlined style={{ color: 'var(--text-3)' }} />}
          placeholder="搜索会话…"
          value={searchKw}
          allowClear
          onChange={(e) => setSearchKw(e.target.value)}
        />
      </div>
      <div className="session-list">
        <div className="project-section-head">
          <span>项目</span>
          <button className="icon-btn" title="打开 / 新建项目" onClick={onOpenProject}>
            <PlusOutlined />
          </button>
        </div>
        {recents.length === 0 ? (
          <div className="drawer-empty">暂无项目，点右上角 ＋ 打开</div>
        ) : (
          // 固定按名称排序：点击切换不改变列表顺序，项目位置保持稳定
          [...recents]
            .sort((a, b) => a.name.localeCompare(b.name, 'zh-CN'))
            .map((p) => {
            const open = isExpanded(p.path);
            const isCurrent = p.path === project?.path;
            const items = isCurrent ? sessions : (otherSessions[p.path] ?? []);
            return (
              <div key={p.path} className="tree-project">
                <Dropdown
                  trigger={['contextMenu']}
                  menu={{
                    items: [
                      { key: 'new-chat', icon: <CommentOutlined />, label: '新对话' },
                      { key: 'usage', icon: <BarChartOutlined />, label: 'Token 用量报表' },
                      { key: 'remove', icon: <DeleteOutlined />, label: '移除项目', danger: true },
                    ],
                    onClick: ({ key }) => {
                      if (key === 'new-chat') onNewChat(p.path);
                      if (key === 'usage') navigate(`/usage/${encodeURIComponent(p.path)}`);
                      if (key === 'remove') onRemoveProject(p.path, p.name);
                    },
                  }}
                >
                  <div
                    className={`project-item${isCurrent ? ' current' : ''}`}
                    title={p.path}
                    onClick={() => onProjectClick(p.path)}
                  >
                    <span className="p-arrow">{open ? '▾' : '▸'}</span>
                    <span className="p-icon">{open ? <FolderOpenOutlined /> : <FolderOutlined />}</span>
                    <span className="p-name">{p.name}</span>
                    {isCurrent ? <span className="p-cur">当前</span> : null}
                  </div>
                </Dropdown>
                {open ? (
                  <div className="tree-children">
                    {items.length === 0 ? (
                      <div className="tree-empty">暂无会话</div>
                    ) : (
                      items.map((s) => (
                        <SessionItem
                          key={s.id}
                          session={s}
                          active={isCurrent && s.id === activeId}
                          runFlag={sessionRuns[s.id]}
                          onSelect={(id) => onTreeSelectSession(p.path, id)}
                          onDelete={(id) => onTreeDeleteSession(p.path, id)}
                        />
                      ))
                    )}
                  </div>
                ) : null}
              </div>
            );
          })
        )}
      </div>
      <div className="sidebar-foot">
        <div className="user-card">
          <span className="u-avatar">
            <UserOutlined />
          </span>
          <span>本机用户</span>
        </div>
        <div className="perm-line">{PERM_DESC[permMode]}</div>
      </div>
      <RecycleBinModal open={recycleOpen} onClose={() => setRecycleOpen(false)} />
    </aside>
  );
}
