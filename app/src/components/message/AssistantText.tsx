import { memo, useEffect, useMemo, useState } from 'react';
import ReactMarkdown, { type Components } from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { openExternal } from '../../utils/openExternal';
import logoUrl from '../../assets/logo.png';

interface AssistantTextProps {
  text: string;
  /** 推理模型的思考过程（运行时流式 / 持久化两种来源） */
  thinking?: string;
  streaming?: boolean;
}

/** Markdown 链接渲染：拦截点击改走系统浏览器，避免 WebView 整窗跳转离开应用。
 *  仅放行 http(s)（含 localhost）链接，阻断 javascript: 等危险协议。 */
function MdLink({
  node: _node,
  children,
  href,
  ...rest
}: React.AnchorHTMLAttributes<HTMLAnchorElement> & { node?: unknown }) {
  const safe = typeof href === 'string' && /^https?:\/\//i.test(href);
  return (
    <a
      {...rest}
      href={safe ? href : undefined}
      target="_blank"
      rel="noreferrer noopener"
      onClick={(e) => {
        if (!safe) {
          e.preventDefault();
          return;
        }
        e.preventDefault();
        void openExternal(href as string);
      }}
    >
      {children}
    </a>
  );
}

/** ReactMarkdown 自定义组件映射（稳定引用，避免流式期间重建） */
const mdComponents: Components = {
  a: MdLink,
};

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
    () => <ReactMarkdown remarkPlugins={[remarkGfm]} components={mdComponents}>{text}</ReactMarkdown>,
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
