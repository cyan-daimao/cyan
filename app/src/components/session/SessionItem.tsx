import { useEffect, useRef, useState } from 'react';
import {
  CheckOutlined,
  CloseOutlined,
  EditOutlined,
  LoadingOutlined,
  MessageOutlined,
} from '@ant-design/icons';
import type { SessionRunFlag } from '../../stores/agentStore';
import type { SessionSummaryDTO } from '../../types';

interface SessionItemProps {
  session: SessionSummaryDTO;
  active: boolean;
  /** 运行标记：running/waiting_approval → loading；done/error → 完成/出错提示 */
  runFlag?: SessionRunFlag;
  onSelect: (id: number) => void;
  onDelete: (id: number) => void;
  /** 触发行内重命名（父组件控制实际保存逻辑） */
  onRename: (id: number, title: string) => Promise<boolean>;
}

/** 会话列表项：图标 + 标题 + 右侧运行指示 + 悬停删除/编辑 */
export function SessionItem({
  session,
  active,
  runFlag,
  onSelect,
  onDelete,
  onRename,
}: SessionItemProps) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(session.title);
  const [saving, setSaving] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  // 进入编辑态：聚焦 + 全选
  useEffect(() => {
    if (editing) {
      setDraft(session.title);
      requestAnimationFrame(() => {
        const el = inputRef.current;
        if (el) {
          el.focus();
          el.select();
        }
      });
    }
  }, [editing, session.title]);

  /** 退出编辑（成功/取消统一入口） */
  const exit = (next?: string) => {
    setEditing(false);
    if (next !== undefined) setDraft(next);
  };

  /** 保存：委托父组件；空值或失败保留草稿并继续编辑 */
  const save = async () => {
    const next = draft.trim();
    if (!next) {
      // 空值不退出编辑，让用户改
      inputRef.current?.focus();
      return;
    }
    if (next === session.title.trim()) {
      exit();
      return;
    }
    setSaving(true);
    const ok = await onRename(session.id, next);
    setSaving(false);
    if (ok) exit(next);
    else inputRef.current?.focus();
  };

  if (editing) {
    return (
      <div className={`session-item editing${active ? ' active' : ''}`}>
        <span className="s-avatar">
          <MessageOutlined />
        </span>
        <input
          ref={inputRef}
          className="s-rename-input"
          value={draft}
          maxLength={80}
          autoCapitalize="off"
          autoCorrect="off"
          spellCheck={false}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            // IME 组合中（拼音候选未上屏）不抢按键
            if (e.nativeEvent.isComposing || e.nativeEvent.keyCode === 229) return;
            if (e.key === 'Enter') {
              e.preventDefault();
              void save();
            } else if (e.key === 'Escape') {
              e.preventDefault();
              exit();
            }
          }}
          onBlur={() => {
            // 失焦自动保存（避免误改）；若保存失败，useEffect 不会重入，编辑态已被 exit 关闭 → 用户可再次触发
            if (saving) return;
            if (draft.trim() && draft.trim() !== session.title.trim()) {
              void save();
            } else {
              exit();
            }
          }}
          onClick={(e) => e.stopPropagation()}
        />
        {saving ? (
          <span className="s-saving" title="保存中">
            <LoadingOutlined spin />
          </span>
        ) : null}
      </div>
    );
  }

  return (
    <div
      className={`session-item${active ? ' active' : ''}`}
      onClick={() => onSelect(session.id)}
    >
      <span className="s-avatar">
        <MessageOutlined />
      </span>
      <span className="s-title" title={session.title}>
        {session.title}
      </span>
      {runFlag === 'running' || runFlag === 'waiting_approval' ? (
        <span className="s-run" title={runFlag === 'waiting_approval' ? '等待审批' : '运行中'}>
          <LoadingOutlined spin />
        </span>
      ) : runFlag === 'done' ? (
        <span className="s-done" title="任务已完成">
          <CheckOutlined />
        </span>
      ) : runFlag === 'error' ? (
        <span className="s-err" title="任务出错">
          <CloseOutlined />
        </span>
      ) : null}
      <button
        className="s-edit"
        title="重命名"
        onClick={(e) => {
          e.stopPropagation();
          setEditing(true);
        }}
      >
        <EditOutlined />
      </button>
      <button
        className="s-del"
        title="删除会话"
        onClick={(e) => {
          e.stopPropagation();
          onDelete(session.id);
        }}
      >
        <CloseOutlined />
      </button>
    </div>
  );
}
