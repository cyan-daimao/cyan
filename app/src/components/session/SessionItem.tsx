import type { SessionSummaryDTO } from '../../types';
import { sessionEmoji } from '../../utils/emoji';

interface SessionItemProps {
  session: SessionSummaryDTO;
  active: boolean;
  onSelect: (id: number) => void;
  onDelete: (id: number) => void;
}

/** 会话列表项：emoji 头像 + 标题 + 悬停删除 */
export function SessionItem({ session, active, onSelect, onDelete }: SessionItemProps) {
  return (
    <div
      className={`session-item${active ? ' active' : ''}`}
      onClick={() => onSelect(session.id)}
    >
      <span className="s-avatar">{sessionEmoji(session.title)}</span>
      <span className="s-title" title={session.title}>
        {session.title}
      </span>
      <button
        className="s-del"
        title="删除会话"
        onClick={(e) => {
          e.stopPropagation();
          onDelete(session.id);
        }}
      >
        ✕
      </button>
    </div>
  );
}
