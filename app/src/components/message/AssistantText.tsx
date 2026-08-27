import { useEffect, useState } from 'react';
import logoUrl from '../../assets/logo.png';

interface AssistantTextProps {
  text: string;
  /** 推理模型的思考过程（运行时流式 / 持久化两种来源） */
  thinking?: string;
  streaming?: boolean;
}

/** 助手消息：头像 + 可折叠思考过程区块 + 文本，流式时尾部闪烁光标 */
export function AssistantText({ text, thinking, streaming }: AssistantTextProps) {
  // 流式期间思考块默认展开，结束后默认折叠为摘要行
  const [thinkingOpen, setThinkingOpen] = useState(true);
  useEffect(() => {
    if (!streaming) setThinkingOpen(false);
  }, [streaming]);

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
        {text}
        {streaming && (text || !thinking) ? <span className="cursor" /> : null}
      </div>
    </div>
  );
}
