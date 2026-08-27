import { useEffect } from 'react';
import { App as AntdApp, ConfigProvider } from 'antd';
import zhCN from 'antd/locale/zh_CN';
import { themeConfig } from './theme';
import { AppRoutes } from './routes';
import { FeedbackBinder } from './utils/feedback';
import { listenAgentEvents } from './services/agent';
import { useAgentStore } from './stores/agentStore';

export default function App() {
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

  return (
    <ConfigProvider locale={zhCN} theme={themeConfig}>
      <AntdApp>
        <FeedbackBinder />
        <AppRoutes />
      </AntdApp>
    </ConfigProvider>
  );
}
