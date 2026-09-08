import { createHashRouter, Navigate, RouterProvider } from 'react-router-dom';
import ChatPage from '../pages/chat';
import UsagePage from '../pages/usage';
import { BrowserPanel } from '../pages/browser/BrowserPanelPage';

/**
 * 桌面单窗口两级路由（TECH_DESIGN 2.3）：
 * `/` → `/chat`；`/usage/:projectPath` → Token 用量报表；
 * `/browser-popout` → 浏览器悬浮窗专用（browser_popout 命令创建的子窗口加载；
 *   主窗口的浏览器面板是 ChatPage 内的右侧 panel 组件，不走路由）；
 * 设置 / 项目 / 预览 / diff / 确认均为 Modal/Drawer 不占路由。
 * HashRouter 适配 Tauri 自定义协议加载。
 */
const router = createHashRouter([
  { path: '/', element: <Navigate to="/chat" replace /> },
  { path: '/chat', element: <ChatPage /> },
  { path: '/usage/:projectPath', element: <UsagePage /> },
  { path: '/browser-popout', element: <BrowserPanel variant="floating" /> },
  { path: '*', element: <Navigate to="/chat" replace /> },
]);

export function AppRoutes() {
  return <RouterProvider router={router} />;
}
