import { Modal, Tabs } from 'antd';
import { ModelsTab } from './ModelsTab';
import { PermsTab } from './PermsTab';
import { ThemeTab } from './ThemeTab';
import { AboutTab } from './AboutTab';

export type SettingsTabKey = 'models' | 'perms' | 'theme' | 'about';

interface SettingsModalProps {
  open: boolean;
  tab: SettingsTabKey;
  onTabChange: (tab: SettingsTabKey) => void;
  onClose: () => void;
}

/** 设置弹窗：模型配置 / 权限规则（全局）/ 主题 / 关于（MCP 已移到「技能 · MCP」面板） */
export function SettingsModal({ open, tab, onTabChange, onClose }: SettingsModalProps) {
  return (
    <Modal open={open} title="设置" width={880} footer={null} onCancel={onClose} destroyOnClose>
      <Tabs
        activeKey={tab}
        onChange={(k) => onTabChange(k as SettingsTabKey)}
        items={[
          { key: 'models', label: '模型配置', children: <ModelsTab /> },
          { key: 'perms', label: '权限规则', children: <PermsTab /> },
          { key: 'theme', label: '主题', children: <ThemeTab /> },
          { key: 'about', label: '关于', children: <AboutTab /> },
        ]}
      />
    </Modal>
  );
}
