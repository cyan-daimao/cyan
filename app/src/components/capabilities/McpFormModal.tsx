import { useEffect, useState } from 'react';
import { Alert, Form, Input } from 'antd';
import type { McpServerDTO } from '../../types';
import { useConfigStore } from '../../stores/configStore';
import { toast } from '../../utils/feedback';
import { FormModal } from '../common/FormModal';

interface McpFormModalProps {
  open: boolean;
  editing: McpServerDTO | null;
  onClose: () => void;
}

/** MCP 服务器表单（PRD 7.2：name 唯一，command 必填） */
export function McpFormModal({ open, editing, onClose }: McpFormModalProps) {
  const servers = useConfigStore((s) => s.mcpServers);
  const saveMcpServer = useConfigStore((s) => s.saveMcpServer);
  const [form] = Form.useForm<{ name: string; command: string }>();
  const [saving, setSaving] = useState(false);
  const isEdit = editing !== null;

  useEffect(() => {
    if (open) {
      form.setFieldsValue({ name: editing?.name ?? '', command: editing?.command ?? '' });
    }
  }, [open, editing, form]);

  const onOk = async () => {
    let values: { name: string; command: string };
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
          <Input placeholder="例如 github" />
        </Form.Item>
        <Form.Item
          name="command"
          label="启动命令"
          rules={[{ required: true, message: '请输入启动命令' }]}
        >
          <Input.TextArea className="mono" rows={3} placeholder="npx -y @mcp/server-github" />
        </Form.Item>
        <Alert
          type="info"
          showIcon
          message="保存后会尝试拉起子进程并握手，失败会标记为「连接失败」。"
        />
      </Form>
    </FormModal>
  );
}
