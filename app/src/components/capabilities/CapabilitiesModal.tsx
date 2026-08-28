import { Modal, Tabs } from 'antd';
import { ToolsTab } from './ToolsTab';
import { McpTab } from './McpTab';
import { SkillsTab } from './SkillsTab';
import { PluginsTab } from './PluginsTab';

interface CapabilitiesModalProps {
  open: boolean;
  onClose: () => void;
}

/** 能力面板：工具 / MCP 服务器 / 技能 / 插件 */
export function CapabilitiesModal({ open, onClose }: CapabilitiesModalProps) {
  return (
    <Modal open={open} title="技能 · MCP · 插件" width={880} footer={null} onCancel={onClose} destroyOnClose>
      <Tabs
        items={[
          { key: 'tools', label: '工具', children: <ToolsTab /> },
          { key: 'mcp', label: 'MCP 服务器', children: <McpTab /> },
          {
            key: 'skills',
            label: '技能',
            children: <SkillsTab />,
          },
          {
            key: 'plugins',
            label: '插件',
            children: <PluginsTab />,
          },
        ]}
      />
    </Modal>
  );
}
