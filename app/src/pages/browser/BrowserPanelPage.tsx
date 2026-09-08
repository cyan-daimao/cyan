import { useCallback, useEffect, useRef, useState } from 'react';
import { GlobalOutlined, ReloadOutlined } from '@ant-design/icons';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { call } from '../../services/invoke';
import { browserHomeUrl, useConfigStore } from '../../stores/configStore';
import { errText, toast } from '../../utils/feedback';

/** 本组件所在窗口 label（主窗口 main / 悬浮窗 browser-popout；attach/detach 定向用） */
const WIN_LABEL = getCurrentWindow().label;

interface BrowserPanelProps {
  /** 悬浮窗模式下隐藏关闭按钮（关窗即停播，无需页面内控制） */
  variant?: 'panel' | 'floating';
  /** panel 模式的关闭回调（悬浮窗模式不需要） */
  onClose?: () => void;
  /** 弹出为悬浮窗（panel 模式头部按钮；悬浮窗模式不传） */
  onPopout?: () => void;
  /** panel 宽度（拖拽手柄调整；悬浮窗忽略） */
  width?: number;
  /** 拖拽调整宽度回调 */
  onResize?: (width: number) => void;
}

/** 面板容器在窗口内的矩形（CSS px），用于后端定位子 WebView */
interface PanelRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/**
 * 延迟 detach 定时器（模块级，跨挂载共享）。
 * React StrictMode 会同步执行「挂载 → 卸载 → 再挂载」：若卸载时立即 detach，
 * 其异步命令可能与下一次 attach 乱序执行（后到的 detach 把刚建好的视图关掉）。
 * 卸载后延迟一拍再 detach，期间重挂载则取消；快速开关面板同理受益。
 */
let detachTimer: ReturnType<typeof setTimeout> | undefined;

/**
 * 嵌入式浏览器面板：Tauri 原生子 WebView 容器。
 * 子 WebView 由后端 `add_child` 创建并叠加在容器区域（原生渲染，零投屏开销）；
 * 面板只负责控制条 + 容器占位 + 尺寸上报。点击/打字/滚动在视图上原生直通，
 * agent 经 browser_navigate / browser_eval 控制同一视图。
 */
export function BrowserPanel({
  variant = 'panel',
  onClose,
  onPopout,
  width = 400,
  onResize,
}: BrowserPanelProps) {
  const stageRef = useRef<HTMLDivElement | null>(null);
  /** 拖拽调整中（拖拽期间禁帧渲染的 pointer 事件透传） */
  const [dragging, setDragging] = useState(false);
  const [urlDraft, setUrlDraft] = useState('');
  const [title, setTitle] = useState('');
  const [attached, setAttached] = useState(false);

  /** 拖拽手柄：mousedown 起拖，全局 move/up 结算 */
  const startResize = useCallback(
    (e: React.MouseEvent) => {
      if (!onResize) return;
      e.preventDefault();
      setDragging(true);
      const startX = e.clientX;
      const startW = width;
      const onMove = (ev: MouseEvent) => {
        // 上限随窗口宽度：给会话区留 480px，其余都可拉给浏览器
        const maxW = Math.max(720, window.innerWidth - 480);
        const next = Math.min(maxW, Math.max(280, startW + (startX - ev.clientX)));
        onResize(next);
      };
      const onUp = () => {
        setDragging(false);
        window.removeEventListener('mousemove', onMove);
        window.removeEventListener('mouseup', onUp);
      };
      window.addEventListener('mousemove', onMove);
      window.addEventListener('mouseup', onUp);
    },
    [onResize, width],
  );

  // 标题经 browser:title 事件驱动（WebView 原生导航后推送）；
  // 地址栏经 browser:url 事件双向同步（agent 导航/用户点链接都反映到 URL 栏）
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    let unlistenUrl: (() => void) | undefined;
    listen<{ title: string }>('browser:title', (e) => {
      if (disposed) return;
      setTitle(e.payload.title);
    })
      .then((u) => {
        if (disposed) u();
        else unlisten = u;
      })
      .catch(() => {});
    listen<{ url: string }>('browser:url', (e) => {
      if (disposed) return;
      setUrlDraft(e.payload.url);
    })
      .then((u) => {
        if (disposed) u();
        else unlistenUrl = u;
      })
      .catch(() => {});
    return () => {
      disposed = true;
      unlisten?.();
      unlistenUrl?.();
    };
  }, []);

  // 挂载即 attach（实测容器 rect 后创建子 WebView）；卸载即 detach。
  // ResizeObserver 跟踪尺寸变化重定位子视图。
  useEffect(() => {
    const stage = stageRef.current;
    if (!stage) return;
    let disposed = false;
    // 重挂载：取消上一次卸载遗留的延迟 detach
    clearTimeout(detachTimer);

    const sync = () => {
      if (disposed) return;
      const r = stage.getBoundingClientRect();
      const rect: PanelRect = {
        x: r.left,
        y: r.top,
        width: r.width,
        height: r.height,
      };
      if (rect.width < 200 || rect.height < 150) return;
      void call('browser_attach', {
        cmd: {
          windowLabel: WIN_LABEL,
          x: rect.x,
          y: rect.y,
          width: rect.width,
          height: rect.height,
          // 默认主页：仅新建视图时加载，重定位不影响当前页面
          homeUrl: browserHomeUrl(useConfigStore.getState().browserHome),
        },
      })
        .then(() => {
          if (!disposed) setAttached(true);
        })
        .catch((err) => {
          if (!disposed) toast.error(`挂载浏览器失败：${errText(err)}`);
        });
    };

    // 首次挂载（延迟到布局完成后）
    const raf = requestAnimationFrame(sync);
    // 尺寸/位置变化时重定位
    const observer = new ResizeObserver(() => sync());
    observer.observe(stage);
    window.addEventListener('resize', sync);
    return () => {
      disposed = true;
      cancelAnimationFrame(raf);
      observer.disconnect();
      window.removeEventListener('resize', sync);
      // 延迟 detach：StrictMode 双挂载/快速重开会在下一拍取消它
      clearTimeout(detachTimer);
      detachTimer = setTimeout(() => {
        void call('browser_detach', { windowLabel: WIN_LABEL }).catch(() => {});
      }, 200);
      setAttached(false);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const onNavigate = () => {
    const url = urlDraft.trim();
    if (!url) return;
    void call('browser_navigate', { url })
      .then(() => toast.success(`正在打开 ${url}`))
      .catch((err) => toast.error(`导航失败：${errText(err)}`));
  };

  const onRefresh = () => {
    const url = urlDraft.trim();
    if (url) void call('browser_navigate', { url }).catch(() => {});
  };

  const isPanel = variant === 'panel';

  return (
    <aside
      className={`browser-panel${isPanel ? ' as-panel' : ' as-floating'}${dragging ? ' resizing' : ''}`}
      style={isPanel ? { width } : undefined}
    >
      {isPanel && onResize ? (
        <div className="bp-resize-handle" title="拖拽调整宽度" onMouseDown={startResize} />
      ) : null}
      <div className="bp-toolbar">
        <GlobalOutlined className="bp-brand" />
        <input
          className="bp-url mono"
          value={urlDraft}
          placeholder="输入网址回车打开"
          autoCapitalize="off"
          autoCorrect="off"
          spellCheck={false}
          onChange={(e) => setUrlDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') onNavigate();
          }}
        />
        <button className="icon-btn" title="刷新" onClick={onRefresh} disabled={!urlDraft.trim()}>
          <ReloadOutlined />
        </button>
        {isPanel && onPopout ? (
          <button className="icon-btn" title="弹出为悬浮窗" onClick={onPopout}>
            ⇱
          </button>
        ) : null}
        {isPanel && onClose ? (
          <button className="icon-btn" title="关闭浏览器面板" onClick={onClose}>
            ✕
          </button>
        ) : null}
      </div>
      <div className="bp-title" title={title || urlDraft}>
        {title || urlDraft || '等待输入网址…'}
      </div>
      {/* 容器占位：子 WebView 由后端叠加在此区域；面板自身只负责控制条与提示 */}
      <div ref={stageRef} className="bp-stage">
        {!attached ? (
          <div className="bp-empty">
            <p>浏览器面板</p>
            <p className="bp-empty-hint">
              在上方地址栏输入网址回车打开——原生内嵌视图（无投屏延迟），
              你可以直接点击、打字、滚动，与 agent 共同操作同一浏览器。
            </p>
          </div>
        ) : null}
      </div>
    </aside>
  );
}
