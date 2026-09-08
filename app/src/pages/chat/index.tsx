import { useCallback, useEffect, useRef, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { RightOutlined } from '@ant-design/icons';
import { listen } from '@tauri-apps/api/event';
import { Sidebar } from './components/Sidebar';
import { Topbar } from './components/Topbar';
import { EmptyState } from './components/EmptyState';
import { MessageList } from './components/MessageList';
import { InputArea } from './components/InputArea';
import { FilePanel } from './components/FilePanel';
import { BrowserPanel } from '../browser/BrowserPanelPage';
import { TaskDrawer } from '../../components/drawer/TaskDrawer';
import { ProjectModal } from '../../components/project/ProjectModal';
import { SettingsModal } from '../../components/settings/SettingsModal';
import type { SettingsTabKey } from '../../components/settings/SettingsModal';
import { CapabilitiesModal } from '../../components/capabilities/CapabilitiesModal';
import { useWindowWidth } from './hooks/useResponsive';
import { useSessionStore } from '../../stores/sessionStore';
import { useAgentStore } from '../../stores/agentStore';
import { useProjectStore } from '../../stores/projectStore';
import { useConfigStore } from '../../stores/configStore';
import { isBusy } from '../../utils/guard';
import { confirmDanger, toast } from '../../utils/feedback';

/** 会话主视图：三栏布局（侧栏 / 会话区 / 文件面板）+ 抽屉与弹窗编排 */
export default function ChatPage() {
  const width = useWindowWidth();
  const [, setSearchParams] = useSearchParams();

  const [sidebarCollapsed, setSidebarCollapsed] = useState(() => window.innerWidth < 860);
  const [filePanelOpen, setFilePanelOpen] = useState(true);
  /** 浏览器面板（右栏；与文件面板共存，窄屏时优先保浏览器） */
  const [browserPanelOpen, setBrowserPanelOpen] = useState(false);
  /** 浏览器面板宽度（拖拽调整；最小 280px，最大给会话区留 480px） */
  const [browserPanelWidth, setBrowserPanelWidth] = useState(400);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [projectOpen, setProjectOpen] = useState(false);
  const [skillsOpen, setSkillsOpen] = useState(false);
  const [settings, setSettings] = useState<{ open: boolean; tab: SettingsTabKey }>({
    open: false,
    tab: 'models',
  });

  const [draft, setDraft] = useState('');
  const inputRef = useRef<HTMLTextAreaElement | null>(null);

  const messages = useSessionStore((s) => s.messages);
  const activeId = useSessionStore((s) => s.activeId);
  const project = useProjectStore((s) => s.current);

  /* 窄屏（<860px）侧栏默认收起 */
  useEffect(() => {
    if (width < 860) setSidebarCollapsed(true);
  }, [width]);

  /* 同步浏览器面板宽度到全局 CSS 变量：全局确认弹窗（feedback-avoid-browser）据此让位 */
  useEffect(() => {
    document.documentElement.style.setProperty(
      '--browser-panel-w',
      browserPanelOpen ? `${browserPanelWidth}px` : '0px',
    );
  }, [browserPanelOpen, browserPanelWidth]);

  /* agent 浏览器工具需要可视视图时，后端发事件自动打开浏览器面板 */
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    listen('browser:open-panel', () => {
      if (!disposed) setBrowserPanelOpen(true);
    })
      .then((u) => {
        if (disposed) u();
        else unlisten = u;
      })
      .catch(() => {});
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  /* 启动初始化：项目 → 会话 → 配置 */
  useEffect(() => {
    void (async () => {
      await useProjectStore.getState().init();
      const cur = useProjectStore.getState().current;
      if (cur) {
        await useSessionStore.getState().loadSessions(cur.path);
        // ?s=<id> 刷新恢复（会话 id 为后端 i64）
        const sid = new URLSearchParams(window.location.hash.split('?')[1]).get('s');
        const sidNum = sid === null ? NaN : Number(sid);
        if (Number.isInteger(sidNum)) {
          const dto = await useSessionStore.getState().openSession(sidNum);
          if (dto) useAgentStore.getState().resetForSession(dto);
        }
      }
      void useConfigStore.getState().loadAll();
    })();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  /* Esc 中断（弹窗/抽屉内不触发，避免与关闭快捷键冲突） */
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape' || !isBusy()) return;
      const el = e.target as HTMLElement | null;
      if (el?.closest?.('.ant-modal-wrap, .ant-drawer')) return;
      void useAgentStore.getState().interrupt();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  const focusInput = useCallback(() => {
    setTimeout(() => inputRef.current?.focus(), 0);
  }, []);

  /* ---- 侧栏动作 ---- */

  const onOpenProject = useCallback(() => {
    // 多项目并发：切换项目不阻塞后台运行中的会话
    setProjectOpen(true);
  }, []);

  const onOpenSettings = useCallback((tab: SettingsTabKey) => {
    setSettings({ open: true, tab });
  }, []);

  const onSelectSession = useCallback(
    (id: number) => {
      // 多会话并发：允许随时切换，运行中的会话在后台继续
      if (id === useSessionStore.getState().activeId) return;
      useAgentStore.getState().clearSessionFlag(id);
      void useSessionStore
        .getState()
        .openSession(id)
        .then((dto) => {
          if (dto) {
            useAgentStore.getState().resetForSession(dto);
            setSearchParams({ s: String(id) });
          }
        });
      if (window.innerWidth < 860) setSidebarCollapsed(true);
    },
    [setSearchParams],
  );

  const onDeleteSession = useCallback((id: number) => {
    const flag = useAgentStore.getState().sessionRuns[id];
    if (flag === 'running' || flag === 'waiting_approval') {
      toast.warning('该会话任务运行中，请先停止再删除');
      return;
    }
    const title = useSessionStore.getState().sessions.find((s) => s.id === id)?.title ?? '';
    confirmDanger({
      title: '删除会话',
      content: (
        <span>
          确定删除会话 <b>{title}</b> 及其全部消息记录吗？此操作不可恢复。
        </span>
      ),
      okText: '删除',
      onOk: async () => {
        await useSessionStore.getState().deleteSession(id);
        useAgentStore.getState().resetForSession();
        toast.success('会话已删除');
      },
    });
  }, []);

  /* ---- 输入框外部填入 ---- */

  const fillDraft = useCallback(
    (text: string) => {
      setDraft(text);
      focusInput();
    },
    [focusInput],
  );

  const onReference = useCallback(
    (relPath: string) => {
      setDraft((prev) => (prev ? `${prev} @${relPath} ` : `@${relPath} `));
      focusInput();
    },
    [focusInput],
  );

  const showFilePanel = filePanelOpen && width >= 1100;
  const mobileMask = width < 860 && !sidebarCollapsed;

  return (
    <div className="app-shell">
      {sidebarCollapsed ? (
        <button className="fab-expand" title="展开侧边栏" onClick={() => setSidebarCollapsed(false)}>
          <RightOutlined />
        </button>
      ) : null}
      <Sidebar
        collapsed={sidebarCollapsed}
        onToggle={() => setSidebarCollapsed((v) => !v)}
        onOpenProject={onOpenProject}
        onOpenSettings={onOpenSettings}
        onOpenSkills={() => setSkillsOpen(true)}
        onToggleBrowser={() => setBrowserPanelOpen((v) => !v)}
        onSelectSession={onSelectSession}
        onDeleteSession={onDeleteSession}
      />
      {mobileMask ? (
        <div className="mobile-mask" onClick={() => setSidebarCollapsed(true)} />
      ) : null}

      <div className="app-main">
        <Topbar
          showFiles={width >= 1100}
          filesActive={showFilePanel}
          onToggleFiles={() => setFilePanelOpen((v) => !v)}
          onOpenDrawer={() => setDrawerOpen(true)}
          browserActive={browserPanelOpen}
          onToggleBrowser={() => setBrowserPanelOpen((v) => !v)}
        />
        <div className="body-wrap">
          <main className="chat-main">
            <div className="chat-scroll">
              {messages.length === 0 ? (
                <EmptyState onPick={fillDraft} />
              ) : (
                <MessageList key={activeId ?? 'none'} messages={messages} />
              )}
            </div>
            <InputArea draft={draft} onDraftChange={setDraft} inputRef={inputRef} />
          </main>
          {showFilePanel ? (
            <FilePanel
              projectPath={project?.path ?? null}
              projectName={project?.name ?? null}
              onClose={() => setFilePanelOpen(false)}
              onReference={onReference}
            />
          ) : null}
          {browserPanelOpen ? (
            <BrowserPanel
              variant="panel"
              width={browserPanelWidth}
              onResize={setBrowserPanelWidth}
              onClose={() => setBrowserPanelOpen(false)}
              onPopout={() => {
                void import('../../services/agent').then(({ browserPopout }) =>
                  browserPopout()
                    // 弹出成功：主面板关闭（其卸载 detach 带 main 标签，不会关掉已迁走的视图）
                    .then(() => setBrowserPanelOpen(false))
                    .catch(() => toast.error('弹出悬浮窗失败')),
                );
              }}
            />
          ) : null}
        </div>
      </div>

      <TaskDrawer open={drawerOpen} onClose={() => setDrawerOpen(false)} />
      <ProjectModal open={projectOpen} onClose={() => setProjectOpen(false)} />
      <CapabilitiesModal open={skillsOpen} onClose={() => setSkillsOpen(false)} />
      <SettingsModal
        open={settings.open}
        tab={settings.tab}
        onTabChange={(tab) => setSettings((s) => ({ ...s, tab }))}
        onClose={() => setSettings((s) => ({ ...s, open: false }))}
      />
    </div>
  );
}
