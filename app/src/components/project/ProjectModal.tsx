import { useState } from 'react';
import { Alert, Button, Checkbox, Form, Input, Modal, Select, Tabs, Tag } from 'antd';
import { FolderOpenOutlined } from '@ant-design/icons';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import type { ProjectTemplate } from '../../types';
import { useProjectStore } from '../../stores/projectStore';
import { toast } from '../../utils/feedback';

/** 脚手架模板（与后端 domain project.rs 一致：empty / rust / node） */
const TEMPLATES: { value: ProjectTemplate; label: string; desc: string }[] = [
  { value: 'empty', label: '空项目', desc: '只生成 README.md' },
  { value: 'rust', label: 'Rust', desc: 'Cargo.toml + src 布局的 Rust 工程' },
  { value: 'node', label: 'Node', desc: 'package.json + 入口文件的 Node.js 工程' },
];

interface ProjectModalProps {
  open: boolean;
  onClose: () => void;
}

/** 项目弹窗：打开文件夹 / 新建项目 两个 Tab */
export function ProjectModal({ open, onClose }: ProjectModalProps) {
  const current = useProjectStore((s) => s.current);
  const recents = useProjectStore((s) => s.recents);
  const openProject = useProjectStore((s) => s.open);
  const createProject = useProjectStore((s) => s.create);

  const [tab, setTab] = useState<'open' | 'create'>('open');
  const [opening, setOpening] = useState(false);
  const [creating, setCreating] = useState(false);
  const [form] = Form.useForm<{ name: string; parent: string; template: ProjectTemplate; gitInit: boolean }>();

  const doOpen = async (p: string) => {
    // 多项目并发：切换项目不阻塞后台运行中的会话
    if (p === current?.path) {
      onClose();
      return;
    }
    setOpening(true);
    const ok = await openProject(p);
    setOpening(false);
    if (ok) onClose();
  };

  /** 弹出系统文件夹选择器，选中后直接打开 */
  const pickAndOpen = async () => {
    const picked = await openDialog({ directory: true, multiple: false, title: '选择项目文件夹' });
    if (typeof picked === 'string') void doOpen(picked);
  };

  /** 为「新建项目」的所在目录选择文件夹，回填到表单 */
  const pickParent = async () => {
    const picked = await openDialog({ directory: true, multiple: false, title: '选择所在目录' });
    if (typeof picked === 'string') form.setFieldValue('parent', picked);
  };

  const onCreate = async () => {
    let values: { name: string; parent: string; template: ProjectTemplate; gitInit: boolean };
    try {
      values = await form.validateFields();
    } catch {
      return;
    }
    const parent = values.parent.trim().replace(/\/+$/, '');
    const fullPath = `${parent}/${values.name.trim()}`;
    if (recents.some((r) => r.path === fullPath)) {
      form.setFields([{ name: 'name', errors: ['该目录下已存在同名项目'] }]);
      return;
    }
    setCreating(true);
    const p = await createProject(values.name.trim(), parent, values.template, values.gitInit);
    setCreating(false);
    if (p) {
      const tpl = TEMPLATES.find((t) => t.value === values.template)?.label ?? values.template;
      toast.success(`已用「${tpl}」模板创建 ${p.name}${values.gitInit ? '，并初始化 git 仓库' : ''}`);
      onClose();
    }
  };

  return (
    <Modal open={open} title="项目" width={640} footer={null} onCancel={onClose} destroyOnClose>
      <Tabs
        activeKey={tab}
        onChange={(k) => setTab(k as 'open' | 'create')}
        items={[
          {
            key: 'open',
            label: '打开文件夹',
            children: (
              <div>
                <Alert
                  type="info"
                  showIcon
                  style={{ marginBottom: 12 }}
                  message="项目即工作目录：Agent 的文件读写、Shell 命令、会话归档都限定在项目目录内，防止越权操作其他文件。"
                />
                <Button
                  type="primary"
                  icon={<FolderOpenOutlined />}
                  loading={opening}
                  block
                  size="large"
                  onClick={() => void pickAndOpen()}
                >
                  选择文件夹打开…
                </Button>
                <div className="section-title">最近项目</div>
                {recents.length === 0 ? (
                  <div className="drawer-empty">暂无最近项目</div>
                ) : (
                  recents.map((p) => (
                    <div
                      key={p.path}
                      className={`recent-item${p.path === current?.path ? ' current' : ''}`}
                      onClick={() => void doOpen(p.path)}
                    >
                      <span className="r-icon">
                        <FolderOpenOutlined />
                      </span>
                      <div className="r-main">
                        <div className="r-name">
                          {p.name} {p.path === current?.path ? <Tag color="processing">当前</Tag> : null}
                        </div>
                        <div className="r-path mono">{p.path}</div>
                      </div>
                      <span className="r-meta">{p.lastOpenedAt ?? ''}</span>
                    </div>
                  ))
                )}
              </div>
            ),
          },
          {
            key: 'create',
            label: '新建项目',
            children: (
              <Form
                form={form}
                layout="vertical"
                initialValues={{ parent: '~/Documents/workspace', template: 'empty', gitInit: true }}
              >
                <Form.Item
                  name="name"
                  label="项目名称"
                  rules={[
                    { required: true, message: '请输入项目名称' },
                    {
                      pattern: /^[a-zA-Z0-9._-]+$/,
                      message: '名称只能包含字母、数字、- _ .',
                    },
                  ]}
                >
                  <Input placeholder="例如 my-new-app" />
                </Form.Item>
                <Form.Item
                  name="parent"
                  label="所在目录"
                  rules={[
                    { required: true, message: '请选择所在目录' },
                    { pattern: /^[~\/]/, message: '目录需以 ~ 或 / 开头' },
                  ]}
                >
                  <Input
                    className="mono"
                    readOnly
                    placeholder="点击右侧按钮选择文件夹"
                    suffix={
                      <Button size="small" icon={<FolderOpenOutlined />} onClick={() => void pickParent()}>
                        浏览…
                      </Button>
                    }
                  />
                </Form.Item>
                <Form.Item name="template" label="脚手架模板" rules={[{ required: true }]}>
                  <Select
                    options={TEMPLATES.map((t) => ({ value: t.value, label: `${t.label} — ${t.desc}` }))}
                  />
                </Form.Item>
                <Form.Item name="gitInit" valuePropName="checked" style={{ marginBottom: 20 }}>
                  <Checkbox>初始化 git 仓库（推荐，checkpoint 回滚依赖它）</Checkbox>
                </Form.Item>
                <div style={{ display: 'flex', justifyContent: 'flex-end' }}>
                  <Button type="primary" loading={creating} onClick={() => void onCreate()}>
                    创建并打开
                  </Button>
                </div>
              </Form>
            ),
          },
        ]}
      />
    </Modal>
  );
}
