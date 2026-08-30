import { useEffect, useState } from 'react';
import { Alert, Button, Form, Input, Segmented, Space } from 'antd';
import type { McpServerDTO, McpTransport } from '../../types';
import { useConfigStore } from '../../stores/configStore';
import { toast } from '../../utils/feedback';
import { FormModal } from '../common/FormModal';

interface McpFormModalProps {
  open: boolean;
  editing: McpServerDTO | null;
  onClose: () => void;
}

interface FormValues {
  transport: McpTransport;
  name: string;
  command: string;
  headersText: string;
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

/** 美化 JSON 对象文本（无法解析时原样返回，交给校验报错） */
function prettyHeaders(text: string): string {
  try {
    const v = JSON.parse(text);
    if (v && typeof v === 'object' && !Array.isArray(v)) {
      return Object.keys(v).length ? JSON.stringify(v, null, 2) : '{}';
    }
  } catch {
    /* 交给后端/前端校验报错 */
  }
  return text;
}

/** MCP 服务器表单（PRD 7.2：name 唯一；stdio 命令 / sse 服务地址 + 请求头） */
export function McpFormModal({ open, editing, onClose }: McpFormModalProps) {
  const servers = useConfigStore((s) => s.mcpServers);
  const saveMcpServer = useConfigStore((s) => s.saveMcpServer);
  const [form] = Form.useForm<FormValues>();
  const [saving, setSaving] = useState(false);
  const isEdit = editing !== null;
  const transport = Form.useWatch('transport', form) ?? 'stdio';

  useEffect(() => {
    if (open) {
      form.setFieldsValue({
        transport: editing?.transport ?? 'stdio',
        name: editing?.name ?? '',
        command: editing?.command ?? '',
        headersText: editing ? prettyHeaders(editing.headers || '{}') : '{}',
      });
    }
  }, [open, editing, form]);

  /** 点击预设：一键填充名称与命令（预设均为 stdio） */
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
    // sse 时请求头文本规范化为单行 JSON 再提交
    const headers = values.transport === 'sse' ? prettyHeaders(values.headersText.trim() || '{}') : '{}';
    const ok = await saveMcpServer(editing?.id, values.name.trim(), values.command.trim(), values.transport, headers);
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
      <Form form={form} layout="vertical" preserve={false} initialValues={{ transport: 'stdio' }}>
        <Form.Item name="transport" label="传输方式">
          <Segmented
            options={[
              { label: '本地命令（stdio）', value: 'stdio' },
              { label: '远程服务（SSE）', value: 'sse' },
            ]}
          />
        </Form.Item>
        {!isEdit && transport === 'stdio' && (
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
            placeholder="例如 dbstudio"
          />
        </Form.Item>
        <Form.Item
          name="command"
          label={transport === 'sse' ? '服务地址' : '启动命令'}
          rules={[
            { required: true, message: transport === 'sse' ? '请输入服务地址' : '请输入启动命令' },
            {
              validator: (_, v: string) =>
                values_transport_ok(transport, v)
                  ? Promise.resolve()
                  : Promise.reject(new Error('服务地址必须以 http:// 或 https:// 开头')),
            },
          ]}
        >
          <Input.TextArea
            autoCapitalize="off"
            autoCorrect="off"
            spellCheck={false}
            className="mono"
            rows={transport === 'sse' ? 2 : 3}
            placeholder={transport === 'sse' ? 'http://127.0.0.1:54554/sse' : 'npx -y @mcp/server-github'}
          />
        </Form.Item>
        {transport === 'sse' && (
          <Form.Item
            name="headersText"
            label="请求头（JSON 对象，选填鉴权用）"
            rules={[
              {
                validator: (_, v: string) => headers_ok(v) ? Promise.resolve() : Promise.reject(new Error('必须是合法的 JSON 对象')),
              },
            ]}
          >
            <Input.TextArea
              autoCapitalize="off"
              autoCorrect="off"
              spellCheck={false}
              className="mono"
              rows={3}
              placeholder={'{\n  "Authorization": "Bearer <token>"\n}'}
            />
          </Form.Item>
        )}
        <Alert
          type="info"
          showIcon
          message={
            transport === 'sse'
              ? '保存后会尝试连接远程服务并握手（请求头随连接与调用一并发送），失败会标记为「连接失败」。'
              : '保存后会尝试拉起子进程并握手，失败会标记为「连接失败」。工具以 mcp__<服务器名>__<工具名> 注入。'
          }
        />
      </Form>
    </FormModal>
  );
}

/** sse 服务地址必须是 http(s) URL；stdio 不限 */
function values_transport_ok(transport: McpTransport, v: string | undefined): boolean {
  if (transport !== 'sse') return true;
  const t = (v ?? '').trim();
  return t.startsWith('http://') || t.startsWith('https://');
}

/** 请求头文本必须是合法 JSON 对象（空/空白视为空对象） */
function headers_ok(v: string | undefined): boolean {
  const t = (v ?? '').trim();
  if (!t) return true;
  try {
    const parsed = JSON.parse(t);
    return parsed !== null && typeof parsed === 'object' && !Array.isArray(parsed);
  } catch {
    return false;
  }
}
