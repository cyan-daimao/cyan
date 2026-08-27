import { useState } from 'react';
import { Spin, Tag } from 'antd';
import type { ToolStatus } from '../../types';
import { DiffView } from './DiffView';

/** 工具图标与底色按工具类型区分 */
const TOOL_META: Record<string, { icon: string; cls: string }> = {
  Read: { icon: '📄', cls: 'read' },
  Grep: { icon: '🔍', cls: 'grep' },
  Glob: { icon: '📂', cls: 'grep' },
  Bash: { icon: '💻', cls: 'bash' },
  Edit: { icon: '✏️', cls: 'edit' },
  Write: { icon: '📝', cls: 'edit' },
  FetchURL: { icon: '🌐', cls: 'read' },
  WebSearch: { icon: '🔎', cls: 'grep' },
};

function StatusTag({ status }: { status: ToolStatus }) {
  switch (status) {
    case 'running':
      return (
        <Tag icon={<Spin size="small" />} color="processing">
          执行中
        </Tag>
      );
    case 'ok':
      return <Tag color="success">完成</Tag>;
    case 'error':
      return <Tag color="error">失败</Tag>;
    case 'denied':
      return <Tag>已拒绝</Tag>;
  }
}

interface ToolCardProps {
  tool: string;
  arg: string;
  status: ToolStatus;
  outputType?: 'code' | 'diff' | 'text';
  output?: string;
  note?: string;
}

/** 工具调用卡片：头部可点击展开/收起输出 */
export function ToolCard({ tool, arg, status, outputType, output, note }: ToolCardProps) {
  const [open, setOpen] = useState(false);
  const meta = TOOL_META[tool] ?? { icon: '🔧', cls: 'read' };
  return (
    <div className={`tool-card${open ? ' open' : ''}`}>
      <div className="tool-head" onClick={() => setOpen((v) => !v)}>
        <span className={`tool-icon ${meta.cls}`}>{meta.icon}</span>
        <span className="tool-name">{tool}</span>
        <span className="tool-arg mono" title={arg}>
          {arg}
        </span>
        <span className="tool-status">
          <StatusTag status={status} />
        </span>
        <span className="tool-caret">▶</span>
      </div>
      <div className="tool-body">
        {outputType === 'diff' ? (
          <DiffView diff={output ?? ''} />
        ) : (
          <pre className="mono">{output ?? ''}</pre>
        )}
        {note ? <div className="tool-note">{note}</div> : null}
      </div>
    </div>
  );
}
