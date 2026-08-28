import { useEffect, useState } from 'react';
import { Alert, Form, Input, Radio } from 'antd';
import type { SkillDTO, SkillScope } from '../../types';
import { useSkillStore } from '../../stores/skillStore';
import { useProjectStore } from '../../stores/projectStore';
import { toast } from '../../utils/feedback';
import { FormModal } from '../common/FormModal';

interface SkillFormModalProps {
  open: boolean;
  /** null 表示新增 */
  editing: SkillDTO | null;
  onClose: () => void;
}

interface FormValues {
  scope: SkillScope;
  fileName: string;
  name: string;
  description: string;
  content: string;
}

/** 技能表单：文件名/名称/描述/正文/作用域（PLUGIN_DESIGN 2.5） */
export function SkillFormModal({ open, editing, onClose }: SkillFormModalProps) {
  const skills = useSkillStore((s) => s.skills);
  const save = useSkillStore((s) => s.save);
  const project = useProjectStore((s) => s.current);
  const [form] = Form.useForm<FormValues>();
  const [saving, setSaving] = useState(false);
  const isEdit = editing !== null;

  useEffect(() => {
    if (open) {
      form.setFieldsValue({
        scope:
          editing?.source && editing.source !== 'plugin'
            ? editing.source
            : project
              ? 'project'
              : 'global',
        fileName: editing?.id ?? '',
        name: editing?.name ?? '',
        description: editing?.description ?? '',
        content: editing?.content ?? '',
      });
    }
  }, [open, editing, form, project]);

  const onOk = async () => {
    let values: FormValues;
    try {
      values = await form.validateFields();
    } catch {
      return; // 校验失败：字段红字 + 保留输入
    }
    const scope = values.scope;
    if (scope === 'project' && !project) {
      toast.warning('请先打开一个项目再保存项目级技能');
      return;
    }
    setSaving(true);
    const ok = await save({
      scope,
      fileName: values.fileName.trim(),
      name: values.name.trim(),
      description: values.description.trim(),
      // 表单不含启用开关：新建默认启用，编辑保留原状态
      enabled: editing?.enabled ?? true,
      content: values.content,
      projectPath: scope === 'project' ? project?.path : undefined,
    });
    setSaving(false);
    if (ok) {
      toast.success(isEdit ? '技能已更新' : `技能 ${values.name.trim()} 已创建`);
      onClose();
    }
  };

  return (
    <FormModal
      open={open}
      title={isEdit ? '编辑技能' : '新增技能'}
      confirmLoading={saving}
      onCancel={onClose}
      onOk={() => void onOk()}
    >
      <Form form={form} layout="vertical" preserve={false}>
        <Form.Item name="scope" label="作用域" rules={[{ required: true }]}>
          <Radio.Group
            disabled={isEdit}
            options={[
              { value: 'global', label: '全局（所有项目生效）' },
              { value: 'project', label: '本项目', disabled: !project },
            ]}
          />
        </Form.Item>
        <Form.Item
          name="fileName"
          label="文件名（技能 id，kebab-case，不含扩展名）"
          rules={[
            { required: true, message: '请输入文件名' },
            { pattern: /^[a-zA-Z0-9_-]+$/, message: '只能包含字母、数字、- _' },
            {
              // 同作用域下文件名唯一（编辑时排除自身）
              validator: (_, v: string) => {
                const scope = form.getFieldValue('scope') as SkillScope;
                return skills.some((s) => s.id === v?.trim() && s.source === scope && s.id !== editing?.id)
                  ? Promise.reject(new Error('同作用域下已存在同名文件'))
                  : Promise.resolve();
              },
            },
          ]}
        >
          <Input className="mono" placeholder="例如 weekly-report" disabled={isEdit} />
        </Form.Item>
        <Form.Item name="name" label="名称" rules={[{ required: true, message: '请输入名称' }]}>
          <Input placeholder="例如 周报" />
        </Form.Item>
        <Form.Item
          name="description"
          label="描述"
          rules={[{ required: true, message: '请输入描述' }]}
        >
          <Input placeholder="一句话说明这个技能做什么" />
        </Form.Item>
        <Form.Item
          name="content"
          label="正文（prompt 模板）"
          rules={[{ required: true, message: '请输入正文' }]}
        >
          <Input.TextArea
            className="mono"
            rows={8}
            placeholder="支持 $ARGUMENTS 占位符，用户输入的参数将原样替换"
          />
        </Form.Item>
        <Alert
          type="info"
          showIcon
          message="输入框以 / 开头即可触发技能补全；选中后正文展开到输入框，$ARGUMENTS 由原样保留给用户填写。"
        />
      </Form>
    </FormModal>
  );
}
