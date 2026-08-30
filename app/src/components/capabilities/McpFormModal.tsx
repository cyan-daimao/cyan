import { useEffect, useState } from 'react';
import { Alert, Button, Form, Input, Space } from 'antd';
import type { McpServerDTO } from '../../types';
import { useConfigStore } from '../../stores/configStore';
import { toast } from '../../utils/feedback';
import { FormModal } from '../common/FormModal';

interface McpFormModalProps {
  open: boolean;
  editing: McpServerDTO | null;
  onClose: () => void;
}

interface FormValues {
  name: string;
  command: string;
}

/** 常用 MCP 服务器预设：点击一键填充（对齐 ModelFormModal 风格） */
const PRESET_SERVERS = [
  {
    name: 'websearch',
    command: 'npx -y open-websearch@latest',
    desc: '联网搜索（多引擎免 key：bing/baidu/duckduckgo/csdn/juejin）',
  },
  {
    name: 'github',
    command: 'npx -y @modelcontextprotocol/server-github',
    desc: 'GitHub 仓库/Issue/PR 读写',
  },
  {
    name: 'filesystem',
    command: 'npx -y @modelcontextprotocol/server-filesystem /path/to/dir',
    desc: '扩展目录文件访问（需改路径）',
  },
] as const;

/** MCP 服务器表单（PRD 7.2：name 唯一，command 必填） */
export function McpFormModal({ open, editing, onClose }: McpFormModalProps) {
  const servers = useConfigStore((s) => s.mcpServers);
  const saveMcpServer = useConfigStore((s) => s.saveMcpServer);
  const [form] = Form.useForm<FormValues>();
  const [saving, setSaving] = useState(false);
  const isEdit = editing !== null;

  useEffect(() => {
    if (open) {
      form.setFieldsValue({ name: editing?.name ?? '', command: editing?.command ?? '' });
    }
  }, [open, editing, form]);

  /** 点击预设：一键填充名称与命令 */
  const applyPreset = (p: (typeof PRESET_SERVERS)[number]) => {
    form.setFieldsValue({ name: p.name, command: p.command });
  };

  const onOk = async () => {
    let values: FormValues;
    try {
      values = await form.validateFields();
    } catch {
      return;
    }
    setSaving(true);
    // 保存含握手验证，后端返回最新状态；id 编辑时携带
    const ok = await saveMcpServer(editing?.id, values.name.trim(), values.command.trim());
    setSaving(false);
    if (ok) {
      toast.success(isEdit ? '服务器配置已更新' : `服务器 ${values.name.trim()} 已保存`);
      onClose();
    }
  };

  return (
    <FormModal
      open={open}
      title={isEdit ? '编辑 MCP 服务器' : '新增 MCP 服务器'}
      confirmLoading={saving}
      onCancel={onClose}
      onOk={() => void onOk()}
    >
      <Form form={form} layout="vertical" preserve={false}>
        {!isEdit && (
          <Form.Item label="常用服务器（点击一键填充）">
            <Space size={[8, 8]} wrap>
              {PRESET_SERVERS.map((p) => (
                <Button key={p.name} size="small" title={p.desc} onClick={() => applyPreset(p)}>
                  {p.name}
                </Button>
              ))}
            </Space>
          </Form.Item>
        )}
        <Form.Item
          name="name"
          label="服务器名称"
          rules={[
            { required: true, message: '请输入名称' },
            {
              // 名称全局唯一（编辑时排除自身；后端按 id 更新，允许改名）
              validator: (_, v: string) =>
                servers.some((s) => s.name === v?.trim() && s.id !== editing?.id)
                  ? Promise.reject(new Error('同名服务器已存在'))
                  : Promise.resolve(),
            },
          ]}
        >
          <Input
            autoCapitalize="off"
            autoCorrect="off"
            spellCheck={false}
            placeholder="例如 github"
          />
        </Form.Item>
        <Form.Item
          name="command"
          label="启动命令"
          rules={[{ required: true, message: '请输入启动命令' }]}
        >
          <Input.TextArea
            autoCapitalize="off"
            autoCorrect="off"
            spellCheck={false}
            className="mono"
            rows={3}
            placeholder="npx -y @mcp/server-github"
          />
        </Form.Item>
        <Alert
          type="info"
          showIcon
          message="保存后会尝试拉起子进程并握手，失败会标记为「连接失败」。工具以 mcp__<服务器名>__<工具名> 注入。"
        />
      </Form>
    </FormModal>
  );
}
