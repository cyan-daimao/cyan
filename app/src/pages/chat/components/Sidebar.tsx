import { useEffect } from 'react';
import { Input } from 'antd';
import { SearchOutlined } from '@ant-design/icons';
import { SessionList } from '../../../components/session/SessionList';
import { useSessionStore } from '../../../stores/sessionStore';
import { useProjectStore } from '../../../stores/projectStore';
import { useConfigStore } from '../../../stores/configStore';
import type { PermMode } from '../../../types';

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
  onOpenSettings: (tab: 'models' | 'mcp') => void;
  onSelectSession: (id: number) => void;
  onDeleteSession: (id: number) => void;
}

/** 通高侧栏：品牌行 / 导航 / 会话搜索 / 会话列表 / 底部用户卡片 */
export function Sidebar({
  collapsed,
  onToggle,
  onOpenProject,
  onOpenSettings,
  onSelectSession,
  onDeleteSession,
}: SidebarProps) {
  const sessions = useSessionStore((s) => s.sessions);
  const activeId = useSessionStore((s) => s.activeId);
  const searchKw = useSessionStore((s) => s.searchKw);
  const setSearchKw = useSessionStore((s) => s.setSearchKw);
  const loadSessions = useSessionStore((s) => s.loadSessions);
  const project = useProjectStore((s) => s.current);
  const permMode = useConfigStore((s) => s.permMode);

  // 搜索防抖 300ms，走后端 list_sessions keyword
  useEffect(() => {
    if (!project) return;
    const t = setTimeout(() => {
      void loadSessions(project.path, searchKw || undefined);
    }, 300);
    return () => clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [searchKw, project?.path]);

  return (
    <aside className={`sidebar${collapsed ? ' collapsed' : ''}`}>
      <div className="side-top">
        <div className="side-brand">
          <span>cyan</span>
          <button className="side-collapse" title="收起侧边栏" onClick={onToggle}>
            ‹
          </button>
        </div>
        <button className="nav-item" title="切换 / 新建项目" onClick={onOpenProject}>
          📁 项目<span className="nav-sub mono">{project?.name ?? '未打开'}</span>
        </button>
        <button className="nav-item" onClick={() => onOpenSettings('mcp')}>
          🧩 技能 · MCP
        </button>
        <button className="nav-item" onClick={() => onOpenSettings('models')}>
          ⚙️ 设置
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
        <SessionList
          sessions={sessions}
          activeId={activeId}
          onSelect={onSelectSession}
          onDelete={onDeleteSession}
        />
      </div>
      <div className="sidebar-foot">
        <div className="user-card">
          <span className="u-avatar">😊</span>
          <span>本机用户</span>
        </div>
        <div className="perm-line">{PERM_DESC[permMode]}</div>
      </div>
    </aside>
  );
}
