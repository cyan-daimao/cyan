import { useCallback, useEffect, useRef, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { Sidebar } from './components/Sidebar';
import { Topbar } from './components/Topbar';
import { EmptyState } from './components/EmptyState';
import { MessageList } from './components/MessageList';
import { InputArea } from './components/InputArea';
import { FilePanel } from './components/FilePanel';
import { TaskDrawer } from '../../components/drawer/TaskDrawer';
import { ProjectModal } from '../../components/project/ProjectModal';
import { SettingsModal } from '../../components/settings/SettingsModal';
import type { SettingsTabKey } from '../../components/settings/SettingsModal';
import { useWindowWidth } from './hooks/useResponsive';
import { useSessionStore } from '../../stores/sessionStore';
import { useAgentStore } from '../../stores/agentStore';
import { useProjectStore } from '../../stores/projectStore';
import { useConfigStore } from '../../stores/configStore';
import { guardBusy, isBusy } from '../../utils/guard';
import { confirmDanger, toast } from '../../utils/feedback';

/** 会话主视图：三栏布局（侧栏 / 会话区 / 文件面板）+ 抽屉与弹窗编排 */
export default function ChatPage() {
  const width = useWindowWidth();
  const [, setSearchParams] = useSearchParams();

  const [sidebarCollapsed, setSidebarCollapsed] = useState(() => window.innerWidth < 860);
  const [filePanelOpen, setFilePanelOpen] = useState(true);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [projectOpen, setProjectOpen] = useState(false);
  const [settings, setSettings] = useState<{ open: boolean; tab: SettingsTabKey }>({
    open: false,
    tab: 'models',
  });

  const [draft, setDraft] = useState('');
  const inputRef = useRef<HTMLTextAreaElement | null>(null);

  const messages = useSessionStore((s) => s.messages);
  const project = useProjectStore((s) => s.current);

  /* 窄屏（<860px）侧栏默认收起 */
  useEffect(() => {
    if (width < 860) setSidebarCollapsed(true);
  }, [width]);

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

  const onNewSession = useCallback(() => {
    if (guardBusy('新建会话')) return;
    const cur = useProjectStore.getState().current;
    if (!cur) {
      toast.warning('请先打开一个项目');
      return;
    }
    void useSessionStore
      .getState()
      .createSession(cur.path)
      .then((id) => {
        if (id !== null) {
          useAgentStore.getState().resetForSession();
          setSearchParams({ s: String(id) });
          focusInput();
        }
      });
  }, [focusInput, setSearchParams]);

  const onOpenProject = useCallback(() => {
    if (guardBusy('切换项目')) return;
    setProjectOpen(true);
  }, []);

  const onOpenSettings = useCallback((tab: SettingsTabKey) => {
    setSettings({ open: true, tab });
  }, []);

  const onSelectSession = useCallback(
    (id: number) => {
      if (guardBusy('切换会话')) return;
      if (id === useSessionStore.getState().activeId) return;
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
    if (guardBusy('删除会话')) return;
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
          ›
        </button>
      ) : null}
      <Sidebar
        collapsed={sidebarCollapsed}
        onToggle={() => setSidebarCollapsed((v) => !v)}
        onNewSession={onNewSession}
        onOpenProject={onOpenProject}
        onOpenSettings={onOpenSettings}
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
        />
        <div className="body-wrap">
          <main className="chat-main">
            <div className="chat-scroll">
              {messages.length === 0 ? (
                <EmptyState onPick={fillDraft} />
              ) : (
                <MessageList messages={messages} />
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
        </div>
      </div>

      <TaskDrawer open={drawerOpen} onClose={() => setDrawerOpen(false)} />
      <ProjectModal open={projectOpen} onClose={() => setProjectOpen(false)} />
      <SettingsModal
        open={settings.open}
        tab={settings.tab}
        onTabChange={(tab) => setSettings((s) => ({ ...s, tab }))}
        onClose={() => setSettings((s) => ({ ...s, open: false }))}
      />
    </div>
  );
}
