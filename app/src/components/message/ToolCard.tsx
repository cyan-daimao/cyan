import { useEffect, useRef, useState } from 'react';
import type { ReactNode } from 'react';
import { Spin, Tag } from 'antd';
import {
  CodeOutlined,
  EditOutlined,
  FileTextOutlined,
  GlobalOutlined,
  PictureOutlined,
  SearchOutlined,
  ToolOutlined,
} from '@ant-design/icons';
import { convertFileSrc } from '@tauri-apps/api/core';
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
  BrowserNavigate: { icon: <GlobalOutlined />, cls: 'bash' },
  BrowserSnapshot: { icon: <SearchOutlined />, cls: 'grep' },
  BrowserAction: { icon: <ToolOutlined />, cls: 'bash' },
  BrowserType: { icon: <EditOutlined />, cls: 'bash' },
  BrowserScreenshot: { icon: <PictureOutlined />, cls: 'read' },
  ComputerSnapshot: { icon: <PictureOutlined />, cls: 'grep' },
  ComputerAction: { icon: <ToolOutlined />, cls: 'bash' },
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

/** 从工具输出提取图片路径（浏览器截图 / MCP image 落盘均按此格式输出） */
function extractImagePath(output: string): string | null {
  const m = output.match(/(?:截图已保存|图片已保存)：\s*(\S+\.(?:png|jpe?g|webp|gif))/i);
  return m?.[1] ?? null;
}

/** 工具输出渲染上限（字符）：超出部分折叠，展开后才全量渲染。
 *  长会话里单个工具输出可达几十 KB（如整文件 Read），全量挂 DOM 会让历史翻页极卡。 */
const OUTPUT_RENDER_LIMIT = 4000;

/** 工具输出渲染：含图片路径时渲染图片（asset:// 协议读本地文件）；
 *  超长文本截断预览 + 手动展开完整内容（避免巨 payload 拖垮长列表） */
function ToolOutputBody({ output, outputType }: { output: string; outputType?: 'code' | 'diff' | 'text' }) {
  const [expanded, setExpanded] = useState(false);
  const imgPath = extractImagePath(output);
  if (imgPath) {
    return (
      <div className="tool-shot">
        <a
          className="tool-shot-path mono"
          title={imgPath}
          onClick={(e) => {
            e.preventDefault();
          }}
        >
          <PictureOutlined /> {imgPath}
        </a>
        <img
          className="tool-shot-img"
          src={convertFileSrc(imgPath)}
          alt="工具输出图片"
          loading="lazy"
        />
      </div>
    );
  }
  // 超长输出：默认只渲染前 4000 字符（diff 仍整体保留但同样截断），点击展开全量
  const overflow = output.length > OUTPUT_RENDER_LIMIT && !expanded;
  const shown = overflow ? output.slice(0, OUTPUT_RENDER_LIMIT) : output;
  return (
    <>
      {outputType === 'diff' ? (
        <DiffView diff={shown} />
      ) : (
        <pre className="mono">{shown}</pre>
      )}
      {output.length > OUTPUT_RENDER_LIMIT ? (
        <div className="tool-note tool-expand" onClick={() => setExpanded((v) => !v)}>
          {overflow
            ? `▼ 输出共 ${output.length.toLocaleString()} 字符，已截断预览——点击展开完整内容`
            : '▲ 收起，仅显示截断预览'}
        </div>
      ) : null}
    </>
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
        {!live ? <ToolOutputBody output={output ?? ''} outputType={outputType} /> : null}
        {note ? <div className="tool-note">{note}</div> : null}
      </div>
    </div>
  );
}
