import { useEffect, useMemo } from 'react';
import { App as AntdApp, ConfigProvider, theme as antdTheme } from 'antd';
import zhCN from 'antd/locale/zh_CN';
import enUS from 'antd/locale/en_US';
import { buildTheme } from './theme';
import { AppRoutes } from './routes';
import { FeedbackBinder } from './utils/feedback';
import { listenAgentEvents } from './services/agent';
import { useAgentStore } from './stores/agentStore';
import { useConfigStore } from './stores/configStore';

export default function App() {
  const lang = useConfigStore((s) => s.lang);
  const themeColor = useConfigStore((s) => s.themeColor);
  const bgMode = useConfigStore((s) => s.bgMode);

  /* 订阅 agent:event（单通道），统一进 agentStore.onAgentEvent 分发 */
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    listenAgentEvents((evt) => useAgentStore.getState().onAgentEvent(evt))
      .then((u) => {
        if (disposed) u();
        else unlisten = u;
      })
      .catch(() => {
        // 非 Tauri 环境（纯浏览器调试）无事件通道，忽略
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  /* 主题色同步到自定义 CSS 品牌变量（侧栏选中态/渐变/光标等） */
  useEffect(() => {
    const root = document.documentElement;
    root.style.setProperty('--brand', themeColor);
    root.style.setProperty('--primary', themeColor);
  }, [themeColor]);

  /* 背景主题：data-theme 驱动自定义 CSS 深色覆盖 */
  useEffect(() => {
    document.documentElement.dataset.theme = bgMode;
  }, [bgMode]);

  const theme = useMemo(() => {
    const t = buildTheme(themeColor, bgMode);
    if (bgMode === 'dark') {
      t.algorithm = antdTheme.darkAlgorithm;
    }
    return t;
  }, [themeColor, bgMode]);

  return (
    <ConfigProvider locale={lang === 'en' ? enUS : zhCN} theme={theme}>
      <AntdApp>
        <FeedbackBinder />
        <AppRoutes />
      </AntdApp>
    </ConfigProvider>
  );
}
