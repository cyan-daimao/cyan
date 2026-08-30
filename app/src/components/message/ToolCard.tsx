import { useEffect, useRef, useState } from 'react';
import type { ReactNode } from 'react';
import { Spin, Tag } from 'antd';
import {
  CodeOutlined,
  EditOutlined,
  FileTextOutlined,
  GlobalOutlined,
  SearchOutlined,
  ToolOutlined,
} from '@ant-design/icons';
import type { ToolStatus } from '../../types';
import { DiffView } from './DiffView';

/** 工具图标与底色按工具类型区分（antd icons） */
const TOOL_META: Record<string, { icon: ReactNode; cls: string }> = {
  Read: { icon: <FileTextOutlined />, cls: 'read' },
  Grep: { icon: <SearchOutlined />, cls: 'grep' },
  Glob: { icon: <SearchOutlined />, cls: 'grep' },
  Bash: { icon: <CodeOutlined />, cls: 'bash' },
  Edit: { icon: <EditOutlined />, cls: 'edit' },
  MultiEdit: { icon: <EditOutlined />, cls: 'edit' },
  Write: { icon: <EditOutlined />, cls: 'edit' },
  TodoWrite: { icon: <ToolOutlined />, cls: 'read' },
  WebFetch: { icon: <GlobalOutlined />, cls: 'read' },
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

/** 终端式实时输出框：新输出自动滚到底部；用户上翻时不强拉（回到底部附近后恢复跟随） */
function LiveTerminal({ text }: { text: string }) {
  const ref = useRef<HTMLPreElement>(null);
  /** 用户是否上翻中（不在底部附近） */
  const pinnedUp = useRef(false);

  useEffect(() => {
    const el = ref.current;
    if (el && !pinnedUp.current) el.scrollTop = el.scrollHeight;
  }, [text]);

  return (
    <div className="live-term">
      <div className="live-term-label">
        <Spin size="small" /> 实时输出
      </div>
      <pre
        ref={ref}
        className="mono live-term-body"
        onScroll={(e) => {
          const el = e.currentTarget;
          // 距底部 40px 以内视为跟随模式
          pinnedUp.current = el.scrollHeight - el.scrollTop - el.clientHeight > 40;
        }}
      >
        {text}
      </pre>
    </div>
  );
}

interface ToolCardProps {
  tool: string;
  arg: string;
  status: ToolStatus;
  outputType?: 'code' | 'diff' | 'text';
  output?: string;
  note?: string;
  /** 执行中实时输出（tool_delta 内存态缓冲） */
  liveOutput?: string;
}

/** 工具调用卡片：头部可点击展开/收起输出；执行中有实时输出时强制展开终端块 */
export function ToolCard({ tool, arg, status, outputType, output, note, liveOutput }: ToolCardProps) {
  const [open, setOpen] = useState(false);
  const meta = TOOL_META[tool] ?? { icon: <ToolOutlined />, cls: 'read' };
  const live = status === 'running' && !!liveOutput;
  // 执行中且有实时输出时强制展开（用户仍可点击收起——收起后保持收起直到执行结束）
  const effectiveOpen = open || live;
  return (
    <div className={`tool-card${effectiveOpen ? ' open' : ''}`}>
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
        {live ? <LiveTerminal text={liveOutput} /> : null}
        {!live ? (
          outputType === 'diff' ? (
            <DiffView diff={output ?? ''} />
          ) : (
            <pre className="mono">{output ?? ''}</pre>
          )
        ) : null}
        {note ? <div className="tool-note">{note}</div> : null}
      </div>
    </div>
  );
}
