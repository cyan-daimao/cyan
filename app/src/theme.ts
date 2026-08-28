import type { ThemeConfig } from 'antd';

/**
 * AntD 主题 token：主色可配（设置-主题色），现代中性灰阶 + 大圆角 + 系统字体栈。
 * dark 模式下文本/底色 token 交给 darkAlgorithm，避免浅色文本色压死深色主题。
 */
export function buildTheme(primary: string, mode: 'light' | 'dark' = 'light'): ThemeConfig {
  const lightOnly: ThemeConfig['token'] =
    mode === 'light'
      ? {
          colorText: 'rgba(31, 35, 41, 0.92)',
          colorTextSecondary: 'rgba(78, 85, 96, 1)',
        }
      : {};
  return {
    token: {
      colorPrimary: primary,
      colorInfo: primary,
      colorSuccess: '#22C55E',
      colorWarning: '#F59E0B',
      colorError: '#F43F5E',
      borderRadius: 10,
      fontSize: 14,
      fontFamily:
        "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'PingFang SC', 'Hiragino Sans GB', 'Microsoft YaHei', sans-serif",
      ...lightOnly,
    },
    components: {
      Modal: { borderRadiusLG: 16 },
      Drawer: { borderRadiusLG: 16 },
      Table: mode === 'light' ? { headerBg: '#F7F8F9' } : {},
      Button: { borderRadius: 10 },
      Segmented: mode === 'light' ? { trackBg: '#F1F2F4' } : {},
    },
  };
}
