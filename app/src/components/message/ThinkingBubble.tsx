import logoUrl from '../../assets/logo.png';

/** 「正在思考…」loading 气泡：发送后到首个响应之间展示（三点跳动） */
export function ThinkingBubble() {
  return (
    <div className="msg-row">
      <img className="a-avatar logo-img" src={logoUrl} alt="cyan" />
      <div className="msg-assistant">
        <span className="thinking-dots" aria-label="正在思考">
          <span />
          <span />
          <span />
        </span>
        <span className="thinking-hint">正在思考…</span>
      </div>
    </div>
  );
}
