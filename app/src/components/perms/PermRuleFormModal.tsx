import { useEffect, useState } from 'react';
import { Form, Input, Select } from 'antd';
import type { PermAction, PermRuleDTO, RuleScope } from '../../types';
import { useConfigStore } from '../../stores/configStore';
import { useSessionStore } from '../../stores/sessionStore';
import { useProjectStore } from '../../stores/projectStore';
import { toast } from '../../utils/feedback';
import { FormModal } from '../common/FormModal';

/** 可选工具（与后端 runner.rs builtin_tools 一致） */
export const PERM_TOOLS = [
  'Bash',
  'Edit',
  'MultiEdit',
  'Write',
  'Read',
  'Grep',
  'Glob',
  'WebFetch',
  'TodoWrite',
];

const ACTION_OPTIONS: { value: PermAction; label: string }[] = [
  { value: 'allow', label: 'allow — 静默放行' },
  { value: 'ask', label: 'ask — 弹审批确认' },
  { value: 'deny', label: 'deny — 直接拒绝' },
];

const SCOPE_LABEL: Record<RuleScope, string> = {
  global: '全局 — 所有项目所有会话',
  project: '本项目 — 该项目下所有会话',
  session: '本会话 — 仅当前对话',
};

interface PermRuleFormModalProps {
  open: boolean;
  editing: PermRuleDTO | null;
  /** 新建时可选的作用域（设置页传 ['global']，会话弹窗传 ['project','session']） */
  allowScopes: RuleScope[];
  onClose: () => void;
  /** 保存成功回调（调用方刷新对应列表） */
  onSaved: () => void;
}

/** 权限规则表单（编辑沿用原范围；新建按 allowScopes 选择作用域） */
export function PermRuleFormModal({ open, editing, allowScopes, onClose, onSaved }: PermRuleFormModalProps) {
  const permRules = useConfigStore((s) => s.permRules);
  const globalRules = useConfigStore((s) => s.globalRules);
  const savePermRule = useConfigStore((s) => s.savePermRule);
  const activeId = useSessionStore((s) => s.activeId);
  const project = useProjectStore((s) => s.current);
  const [form] = Form.useForm<{ tool: string; pattern: string; action: PermAction; scope: RuleScope }>();
  const [saving, setSaving] = useState(false);
  const isEdit = editing !== null;

  useEffect(() => {
    if (open) {
      form.setFieldsValue({
        tool: editing?.tool ?? 'Bash',
        pattern: editing?.pattern ?? '',
        action: editing?.action ?? 'ask',
        scope: editing?.scope ?? allowScopes[0],
      });
    }
  }, [open, editing, form, allowScopes]);

  const onOk = async () => {
    let values: { tool: string; pattern: string; action: PermAction; scope: RuleScope };
    try {
      values = await form.validateFields();
    } catch {
      return;
    }
    const scope = isEdit ? undefined : values.scope;
    if (!isEdit && scope === 'session' && activeId === null) {
      toast.warning('请先开始一个对话，再新增会话级规则');
      return;
    }
    if (!isEdit && (scope === 'session' || scope === 'project') && !project) {
      toast.warning('请先打开一个项目');
      return;
    }
    setSaving(true);
    // 编辑保留原顺序；新增追加到规则表末尾（sort 升序匹配）
    const all = [...permRules, ...globalRules];
    const sort = editing?.sort ?? all.reduce((max, r) => Math.max(max, r.sort), -1) + 1;
    const ok = await savePermRule(
      editing?.id,
      scope,
      scope === 'global' ? undefined : project?.id,
      scope === 'session' ? (activeId ?? undefined) : undefined,
      values.tool,
      values.pattern.trim(),
      values.action,
      sort,
    );
    setSaving(false);
    if (ok) {
      toast.success(isEdit ? '规则已更新' : `规则已添加（${SCOPE_LABEL[values.scope].split(' — ')[0]}生效）`);
      onSaved();
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
        {!isEdit && allowScopes.length > 1 ? (
          <Form.Item name="scope" label="作用域" rules={[{ required: true }]}>
            <Select
              options={allowScopes.map((s) => ({ value: s, label: SCOPE_LABEL[s] }))}
            />
          </Form.Item>
        ) : null}
        <Form.Item name="tool" label="工具" rules={[{ required: true }]}>
          <Select options={PERM_TOOLS.map((t) => ({ value: t, label: t }))} />
        </Form.Item>
        <Form.Item
          name="pattern"
          label="匹配模式（glob）"
          rules={[{ required: true, message: '请输入匹配模式' }]}
        >
          <Input
            autoCapitalize="off"
            autoCorrect="off"
            spellCheck={false}
            className="mono"
            placeholder="例如 git push * 或 src/**/*.ts"
          />
        </Form.Item>
        <Form.Item name="action" label="动作" rules={[{ required: true }]}>
          <Select options={ACTION_OPTIONS} />
        </Form.Item>
      </Form>
    </FormModal>
  );
}
