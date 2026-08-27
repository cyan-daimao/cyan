import { useEffect, useState } from 'react';
import { Form, Input, Select } from 'antd';
import type { PermAction, PermRuleDTO } from '../../types';
import { useConfigStore } from '../../stores/configStore';
import { toast } from '../../utils/feedback';
import { FormModal } from '../common/FormModal';

/** 可选工具（PRD 7.3） */
export const PERM_TOOLS = ['Bash', 'Edit', 'Write', 'Read', 'Grep', 'FetchURL', 'WebSearch'];

const ACTION_OPTIONS: { value: PermAction; label: string }[] = [
  { value: 'allow', label: 'allow — 静默放行' },
  { value: 'ask', label: 'ask — 弹审批确认' },
  { value: 'deny', label: 'deny — 直接拒绝' },
];

interface PermRuleFormModalProps {
  open: boolean;
  editing: PermRuleDTO | null;
  onClose: () => void;
}

/** 权限规则表单（PRD 7.3；sort 为后端必填匹配顺序） */
export function PermRuleFormModal({ open, editing, onClose }: PermRuleFormModalProps) {
  const permRules = useConfigStore((s) => s.permRules);
  const savePermRule = useConfigStore((s) => s.savePermRule);
  const [form] = Form.useForm<{ tool: string; pattern: string; action: PermAction }>();
  const [saving, setSaving] = useState(false);
  const isEdit = editing !== null;

  useEffect(() => {
    if (open) {
      form.setFieldsValue({
        tool: editing?.tool ?? 'Bash',
        pattern: editing?.pattern ?? '',
        action: editing?.action ?? 'ask',
      });
    }
  }, [open, editing, form]);

  const onOk = async () => {
    let values: { tool: string; pattern: string; action: PermAction };
    try {
      values = await form.validateFields();
    } catch {
      return;
    }
    setSaving(true);
    // 编辑保留原顺序；新增追加到规则表末尾（sort 升序匹配）
    const sort = editing?.sort ?? permRules.reduce((max, r) => Math.max(max, r.sort), -1) + 1;
    const ok = await savePermRule(editing?.id, values.tool, values.pattern.trim(), values.action, sort);
    setSaving(false);
    if (ok) {
      toast.success(isEdit ? '规则已更新' : '规则已添加');
      onClose();
    }
  };

  return (
    <FormModal
      open={open}
      title={isEdit ? '编辑权限规则' : '新增权限规则'}
      confirmLoading={saving}
      onCancel={onClose}
      onOk={() => void onOk()}
    >
      <Form form={form} layout="vertical" preserve={false}>
        <Form.Item name="tool" label="工具" rules={[{ required: true }]}>
          <Select options={PERM_TOOLS.map((t) => ({ value: t, label: t }))} />
        </Form.Item>
        <Form.Item
          name="pattern"
          label="匹配模式（glob）"
          rules={[{ required: true, message: '请输入匹配模式' }]}
        >
          <Input className="mono" placeholder="例如 git push * 或 src/**/*.ts" />
        </Form.Item>
        <Form.Item name="action" label="动作" rules={[{ required: true }]}>
          <Select options={ACTION_OPTIONS} />
        </Form.Item>
      </Form>
    </FormModal>
  );
}
