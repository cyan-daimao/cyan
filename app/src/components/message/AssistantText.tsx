/** 助手消息：头像 + 文本，流式时尾部闪烁光标 */
import logoUrl from '../../assets/logo.png';

export function AssistantText({ text, streaming }: { text: string; streaming?: boolean }) {
  return (
    <div className="msg-row">
      <img className="a-avatar logo-img" src={logoUrl} alt="cyan" />
      <div className="msg-assistant">
        {text}
        {streaming ? <span className="cursor" /> : null}
      </div>
    </div>
  );
}
