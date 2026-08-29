import { useEffect, useState } from 'react';
import { Button, Form, Input, Select, Space } from 'antd';
import type { ModelDTO, SaveModelRequest } from '../../types';
import { useConfigStore } from '../../stores/configStore';
import { toast } from '../../utils/feedback';
import { FormModal } from '../common/FormModal';

interface ModelFormModalProps {
  open: boolean;
  /** null 表示新增 */
  editing: ModelDTO | null;
  onClose: () => void;
}

interface FormValues {
  name: string;
  provider: string;
  baseUrl: string;
  apiKey?: string;
  contextWindow: number;
}

/** 常用服务商预设：点击后自动填充 provider / baseUrl / contextWindow，用户只需再填 token */
const PRESET_PROVIDERS = [
  { provider: 'Moonshot Kimi', baseUrl: 'https://api.moonshot.cn/v1', contextWindow: 256000, model: 'kimi-k2.5' },
  { provider: 'kimi-codeplan', baseUrl: 'https://api.moonshot.cn/v1', contextWindow: 256000, model: 'k3' },
  { provider: 'Anthropic', baseUrl: 'https://api.anthropic.com', contextWindow: 200000, model: 'claude-sonnet-4.5' },
  { provider: 'OpenAI', baseUrl: 'https://api.openai.com/v1', contextWindow: 400000, model: 'gpt-5.2-codex' },
  { provider: 'DeepSeek', baseUrl: 'https://api.deepseek.com/v1', contextWindow: 128000, model: 'deepseek-v4' },
  { provider: '通义千问', baseUrl: 'https://dashscope.aliyuncs.com/compatible-mode/v1', contextWindow: 131072, model: 'qwen3-coder-plus' },
  { provider: '智谱 GLM', baseUrl: 'https://open.bigmodel.cn/api/paas/v4', contextWindow: 200000, model: 'glm-5' },
  { provider: 'OpenRouter', baseUrl: 'https://openrouter.ai/api/v1', contextWindow: 200000, model: '' },
  { provider: 'Ollama 本地', baseUrl: 'http://localhost:11434/v1', contextWindow: 32768, model: '' },
] as const;

/** 自定义服务商选项值 */
const CUSTOM_PROVIDER = '自定义';

/** 上下文窗口候选项（tokens） */
const CTX_OPTIONS = [32768, 65536, 131072, 200000, 256000, 400000, 1000000].map((v) => ({
  value: v,
  label: `${(v / 1000).toLocaleString()}k tokens`,
}));

const PROVIDER_OPTIONS = [
  ...PRESET_PROVIDERS.map((p) => ({ value: p.provider, label: p.provider })),
  { value: CUSTOM_PROVIDER, label: `${CUSTOM_PROVIDER}（手动填写 Base URL）` },
];

/** 模型表单：校验规则按 PRD 7.1（apiKey 编辑留空不改、baseUrl 必须 http(s)） */
export function ModelFormModal({ open, editing, onClose }: ModelFormModalProps) {
  const models = useConfigStore((s) => s.models);
  const saveModel = useConfigStore((s) => s.saveModel);
  const [form] = Form.useForm<FormValues>();
  const [saving, setSaving] = useState(false);
  const isEdit = editing !== null;

  useEffect(() => {
    if (open) {
      form.setFieldsValue({
        name: editing?.name ?? '',
        provider: editing?.provider ?? undefined,
        baseUrl: editing?.baseUrl ?? '',
        apiKey: '',
        contextWindow: editing?.contextWindow ?? 256000,
      });
    }
  }, [open, editing, form]);

  /** 点击常用服务商按钮：一键填充，名称仅在未填写时带入推荐模型名 */
  const applyPreset = (p: (typeof PRESET_PROVIDERS)[number]) => {
    form.setFieldsValue({
      provider: p.provider,
      baseUrl: p.baseUrl,
      contextWindow: p.contextWindow,
      ...(form.getFieldValue('name')?.trim() || !p.model ? {} : { name: p.model }),
    });
  };

  /** Provider 下拉切换：预设项回填地址与上下文；自定义清空待手填 */
  const onProviderChange = (v: string) => {
    if (v === CUSTOM_PROVIDER) {
      form.setFieldsValue({ baseUrl: '' });
      return;
    }
    const p = PRESET_PROVIDERS.find((x) => x.provider === v);
    if (p) applyPreset(p);
  };

  const onOk = async () => {
    let values: FormValues;
    try {
      values = await form.validateFields();
    } catch {
      return; // 校验失败：字段红字 + 保留输入
    }
    const req: SaveModelRequest = {
      // id 编辑时携带（后端按 id 更新，新增为 upsert）
      id: editing?.id,
      name: values.name.trim(),
      provider: values.provider.trim(),
      baseUrl: values.baseUrl.trim(),
      contextWindow: Number(values.contextWindow),
      // 编辑时留空表示不修改
      apiKey: values.apiKey?.trim() ? values.apiKey.trim() : undefined,
      // 表单不含启停开关：新建默认启用，编辑保留原状态
      enabled: editing ? editing.status === 'enabled' : true,
    };
    setSaving(true);
    const ok = await saveModel(req);
    setSaving(false);
    if (ok) {
      toast.success(isEdit ? '模型配置已更新，连接验证通过' : '模型已添加，连接验证通过');
      onClose();
    }
  };

  return (
    <FormModal
      open={open}
      title={isEdit ? '编辑模型' : '新增模型'}
      confirmLoading={saving}
      onCancel={onClose}
      onOk={() => void onOk()}
    >
      <Form form={form} layout="vertical" preserve={false}>
        {!isEdit && (
          <Form.Item label="常用服务商（点击一键填充，只需再填 API Key）">
            <Space size={[8, 8]} wrap>
              {PRESET_PROVIDERS.map((p) => (
                <Button key={p.provider} size="small" onClick={() => applyPreset(p)}>
                  {p.provider}
                </Button>
              ))}
            </Space>
          </Form.Item>
        )}
        <Form.Item
          name="name"
          label="模型名称"
          rules={[
            { required: true, message: '请输入模型名称' },
            {
              // 名称全局唯一（编辑时排除自身；后端按 id 更新，允许改名）
              validator: (_, v: string) =>
                models.some((m) => m.name === v?.trim() && m.id !== editing?.id)
                  ? Promise.reject(new Error('同名模型已存在'))
                  : Promise.resolve(),
            },
          ]}
        >
          <Input
            autoCapitalize="off"
            autoCorrect="off"
            spellCheck={false}
            placeholder="例如 kimi-k2.5"
          />
        </Form.Item>
        <Form.Item
          name="provider"
          label="Provider"
          rules={[{ required: true, message: '请选择 Provider' }]}
        >
          <Select
            showSearch
            placeholder="选择服务商"
            options={PROVIDER_OPTIONS}
            onChange={onProviderChange}
          />
        </Form.Item>
        <Form.Item
          name="baseUrl"
          label="Base URL"
          rules={[
            { required: true, message: '请输入 Base URL' },
            { pattern: /^https?:\/\//, message: '请输入合法的 http(s) 地址' },
          ]}
        >
          <Input
            autoCapitalize="off"
            autoCorrect="off"
            spellCheck={false}
            className="mono"
            placeholder="https://api.example.com/v1"
          />
        </Form.Item>
        <Form.Item
          name="apiKey"
          label="API Key"
          rules={isEdit ? [] : [{ required: true, message: '请输入 API Key' }]}
        >
          <Input.Password
            autoCapitalize="off"
            autoCorrect="off"
            spellCheck={false}
            className="mono"
            placeholder={editing ? `留空表示不修改（当前 ${editing.maskedKey}）` : 'sk-...'}
          />
        </Form.Item>
        <Form.Item
          name="contextWindow"
          label="上下文窗口"
          rules={[{ required: true, message: '请选择上下文窗口' }]}
        >
          <Select placeholder="选择上下文窗口" options={CTX_OPTIONS} />
        </Form.Item>
      </Form>
    </FormModal>
  );
}
