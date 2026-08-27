import type { ThemeConfig } from 'antd';

/** AntD 主题 token：对齐原型视觉（圆角、品牌蓝 #1677ff → #722ed1） */
export const themeConfig: ThemeConfig = {
  token: {
    colorPrimary: '#1677ff',
    colorInfo: '#1677ff',
    colorSuccess: '#52c41a',
    colorWarning: '#faad14',
    colorError: '#ff4d4f',
    borderRadius: 8,
    fontSize: 14,
  },
  components: {
    Modal: { borderRadiusLG: 16 },
    Drawer: { borderRadiusLG: 16 },
    Table: { headerBg: '#fafafa' },
  },
};
