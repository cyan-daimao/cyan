import { memo, useEffect, useMemo, useState } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import logoUrl from '../../assets/logo.png';

interface AssistantTextProps {
  text: string;
  /** 推理模型的思考过程（运行时流式 / 持久化两种来源） */
  thinking?: string;
  streaming?: boolean;
}

/** 助手消息：头像 + 可折叠思考过程区块 + Markdown 正文，流式时尾部闪烁光标
 *
 * memo：流式时其他历史消息的重渲染不应拖累本组件（ReactMarkdown 解析较重）。
 */
export const AssistantText = memo(function AssistantText({ text, thinking, streaming }: AssistantTextProps) {
  // 流式期间思考块默认展开，结束后默认折叠为摘要行
  const [thinkingOpen, setThinkingOpen] = useState(true);
  useEffect(() => {
    if (!streaming) setThinkingOpen(false);
  }, [streaming]);

  // 仅正文文本变化时重新解析 Markdown（流式每帧变化只影响正在输出的那一条）
  const md = useMemo(
    () => <ReactMarkdown remarkPlugins={[remarkGfm]}>{text}</ReactMarkdown>,
    [text],
  );

  return (
    <div className="msg-row">
      <img className="a-avatar logo-img" src={logoUrl} alt="cyan" />
      <div className="msg-assistant">
        {thinking ? (
          <div className="thinking-block">
            <div
              className={`thinking-head${thinkingOpen ? ' open' : ''}`}
              onClick={() => setThinkingOpen((v) => !v)}
            >
              <span className="thinking-caret">▶</span>
              {thinkingOpen ? '思考过程（点击收起）' : '思考过程（点击展开）'}
            </div>
            {thinkingOpen ? (
              <div className="thinking-content">
                {thinking}
                {streaming && !text ? <span className="cursor" /> : null}
              </div>
            ) : null}
          </div>
        ) : null}
        <div className="md-body">{md}</div>
        {streaming && (text || !thinking) ? <span className="cursor" /> : null}
      </div>
    </div>
  );
});
