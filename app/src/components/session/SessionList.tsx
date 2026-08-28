import { useMemo } from 'react';
import type { SessionSummaryDTO } from '../../types';
import { SessionItem } from './SessionItem';

interface SessionListProps {
  sessions: SessionSummaryDTO[];
  activeId: number | null;
  onSelect: (id: number) => void;
  onDelete: (id: number) => void;
}

/** 会话列表：按 group 字段分组展示 */
export function SessionList({ sessions, activeId, onSelect, onDelete }: SessionListProps) {
  const groups = useMemo(() => {
    const out: { label: string; items: SessionSummaryDTO[] }[] = [];
    for (const s of sessions) {
      const g = out.find((x) => x.label === s.group);
      if (g) g.items.push(s);
      else out.push({ label: s.group, items: [s] });
    }
    return out;
  }, [sessions]);

  if (sessions.length === 0) {
    return <div className="session-empty">暂无会话，直接在下方输入任务即可开始</div>;
  }

  return (
    <>
      {groups.map((g) => (
        <div key={g.label}>
          <div className="session-group-label">{g.label}</div>
          {g.items.map((s) => (
            <SessionItem
              key={s.id}
              session={s}
              active={s.id === activeId}
              onSelect={onSelect}
              onDelete={onDelete}
            />
          ))}
        </div>
      ))}
    </>
  );
}
