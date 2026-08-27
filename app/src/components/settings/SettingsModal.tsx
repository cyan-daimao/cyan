import { Modal, Tabs } from 'antd';
import { ModelsTab } from './ModelsTab';
import { McpTab } from './McpTab';
import { PermsTab } from './PermsTab';
import { AboutTab } from './AboutTab';

export type SettingsTabKey = 'models' | 'mcp' | 'perms' | 'about';

interface SettingsModalProps {
  open: boolean;
  tab: SettingsTabKey;
  onTabChange: (tab: SettingsTabKey) => void;
  onClose: () => void;
}

/** 设置弹窗：模型配置 / MCP 服务器 / 权限规则 / 关于 */
export function SettingsModal({ open, tab, onTabChange, onClose }: SettingsModalProps) {
  return (
    <Modal open={open} title="设置" width={880} footer={null} onCancel={onClose} destroyOnClose>
      <Tabs
        activeKey={tab}
        onChange={(k) => onTabChange(k as SettingsTabKey)}
        items={[
          { key: 'models', label: '模型配置', children: <ModelsTab /> },
          { key: 'mcp', label: 'MCP 服务器', children: <McpTab /> },
          { key: 'perms', label: '权限规则', children: <PermsTab /> },
          { key: 'about', label: '关于', children: <AboutTab /> },
        ]}
      />
    </Modal>
  );
}
