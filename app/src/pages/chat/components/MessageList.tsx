import { memo, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { Button } from 'antd';
import { ArrowDownOutlined, EditOutlined } from '@ant-design/icons';
import type { ChatNode } from '../../../types';
import { useAgentStore } from '../../../stores/agentStore';
import { useSessionStore } from '../../../stores/sessionStore';
import { editMessage } from '../../../services/session';
import { confirmDanger, errText, toast } from '../../../utils/feedback';
import { UserBubble } from '../../../components/message/UserBubble';
import { AssistantText } from '../../../components/message/AssistantText';
import { SystemDivider } from '../../../components/message/SystemDivider';
import { ToolCard } from '../../../components/message/ToolCard';
import { ApprovalCard } from '../../../components/message/ApprovalCard';
import { ThinkingBubble } from '../../../components/message/ThinkingBubble';

/**
 * 消息流（机制移植自 dsh 的 ChatView，deepseek-harness）：
 * - 纯 DOM 渲染：窗口内消息全量挂载（打开会话仅拉尾部一页，历史上滚近顶自动分页 prepend），
 *   避免 virtuoso 估算高度反复修正造成的滚动跳动/白屏，也避免超长会话 DOM 爆炸。
 * - 跟底状态机：以 observedTop 台账区分「程序写入」与「用户滚动」，程序写入不抢滚动权，
 *   用户上滚即解除贴底；配合 ResizeObserver 在流式增高/图片加载时保持贴底。
 * - 分页锚点补偿：prepend 历史前记录首个可见行（锚点），prepend 后按锚点新位置校正
 *   scrollTop，视觉上原行不动。
 */

/** 距底部该阈值内视为「贴底」 */
const FOLLOW_THRESHOLD = 24;

/** 滚动锚点：某可见行（按稳定 key 定位）在滚动视口内的相对位置 */
interface PagingAnchor {
  key: string;
  top: number;
}

/** 按 key 找已渲染的行（不拼选择器，避免注入） */
function anchorRow(list: HTMLElement, key: string): HTMLElement | null {
  for (const row of list.querySelectorAll<HTMLElement>('[data-chat-anchor-key]')) {
    if (row.dataset.chatAnchorKey === key) return row;
  }
  return null;
}

/** 行顶相对滚动视口的位置（视口无关坐标） */
function flowTop(row: HTMLElement, scrollport: HTMLElement): number {
  return row.getBoundingClientRect().top - scrollport.getBoundingClientRect().top;
}

/** 取滚动视口顶部的首个可见行作为锚点行：优先 elementsFromPoint 命中，
 *  布局不可用（测试环境/首帧）时按行序二分兜底 */
function pagingAnchor(list: HTMLElement, scrollport: HTMLElement): HTMLElement | null {
  const viewport = scrollport.getBoundingClientRect();
  if (typeof document.elementsFromPoint === 'function' && viewport.height > 0) {
    const x = viewport.left + viewport.width / 2;
    for (const element of document.elementsFromPoint(x, viewport.top + 1)) {
      const row = element instanceof HTMLElement
        ? element.closest<HTMLElement>('[data-chat-anchor-key]')
        : null;
      if (row !== null && list.contains(row)) return row;
    }
  }
  const rows = list.querySelectorAll<HTMLElement>('[data-chat-flow] > [data-flow-key]:not(:empty)');
  let low = 0;
  let high = rows.length;
  while (low < high) {
    const middle = (low + high) >>> 1;
    if (rows.item(middle).getBoundingClientRect().bottom > viewport.top) high = middle;
    else low = middle + 1;
  }
  const row = rows[low];
  return row !== undefined && row.getBoundingClientRect().top < viewport.bottom ? row : rows[0] ?? null;
}

/** 捕获当前首个可见行（prepend 前调用） */
function scrollPosition(list: HTMLElement, scrollport: HTMLElement): PagingAnchor | null {
  const row = pagingAnchor(list, scrollport);
  const key = row?.dataset.chatAnchorKey;
  if (row === null || key === undefined) return null;
  return { key, top: flowTop(row, scrollport) };
}

interface MessageNodeProps {
  node: ChatNode;
}

/**
 * 消息节点。仅 user 文本消息支持行内编辑（编辑即截断重发）：
 * 编辑第 i 条会物理删除其后所有消息，并自动以新文本重发。
 *
 * memo：流式输出时 messages 数组每帧更新，历史节点对象不可变（浅比较命中）
 * → 每帧只重渲染真正变化的流式节点（含 ReactMarkdown 重解析的仅此一条）。
 * 注意：不要把 index/total 作为 props 传入——每追加一条它们对所有节点都变化，
 * 会击穿 memo 导致长会话流式期间全列表重渲染；截断条数在保存时实时读取。
 */
const MessageNode = memo(function MessageNode({ node }: MessageNodeProps) {
  const decide = useAgentStore((s) => s.decide);
  const activeId = useSessionStore((s) => s.activeId);
  /** 当前会话运行中（含等待审批）时禁用编辑 */
  const busy = useAgentStore((s) => {
    const f = activeId === null ? undefined : s.sessionRuns[activeId];
    return f === 'running' || f === 'waiting_approval';
  });

  const [editing, setEditing] = useState(false);
  const [editText, setEditText] = useState('');
  const [saving, setSaving] = useState(false);

  const editable = node.kind === 'user';

  const startEdit = () => {
    if (!editable || busy) return;
    setEditText(node.text);
    setEditing(true);
  };

  /** 实际执行编辑（截断由调用方确认过） */
  const doSave = async (text: string, truncated: boolean) => {
    setSaving(true);
    try {
      const msgId = await useSessionStore.getState().resolveMessageId(node.id);
      if (msgId === null) {
        toast.error('无法定位该消息，请重新打开会话后重试');
        return;
      }
      const dto = await editMessage(msgId, text);
      // 后端返回截断后的完整会话，直接替换消息列表
      useSessionStore.getState().replaceMessages(dto);
      useAgentStore.getState().resetForSession(dto);
      setEditing(false);
      toast.success('消息已更新');
      // user 消息且发生截断：以新文本重新生成回复（后端 skipAppend，不重插用户消息）
      if (node.kind === 'user' && truncated) {
        await useAgentStore.getState().send(text, { skipAppend: true });
      }
    } catch (e) {
      toast.error(`编辑失败：${errText(e)}`);
    } finally {
      setSaving(false);
    }
  };

  const onSave = () => {
    const text = editText.trim();
    if (!text) {
      toast.warning('内容不能为空');
      return;
    }
    // 实时读取当前列表位置（避免 index/total 作为 props 击穿 memo）
    const msgs = useSessionStore.getState().messages;
    const idx = msgs.findIndex((m) => m.id === node.id);
    const trailing = idx < 0 ? 0 : msgs.length - idx - 1;
    if (trailing === 0) {
      // 编辑最后一条：不动其它消息，无需确认
      void doSave(text, false);
      return;
    }
    confirmDanger({
      title: '编辑消息',
      content: `编辑此消息将永久删除其后 ${trailing} 条消息，不可恢复，并将以新内容重新生成回复。`,
      okText: '确认编辑',
      onOk: () => doSave(text, true),
    });
  };

  /** 行内编辑态：自适应高度 textarea + 保存/取消，Esc 取消 */
  const editBox = (
    <div className="msg-edit-box">
      <textarea
        className="msg-edit-input"
        value={editText}
        autoFocus
        rows={Math.min(10, Math.max(2, editText.split('\n').length + 1))}
        // 中文 IME 下关掉 WebKit 自动首字母大写/纠错
        autoCapitalize="off"
        autoCorrect="off"
        spellCheck={false}
        onChange={(e) => setEditText(e.target.value)}
        onKeyDown={(e) => {
          // Esc 取消时避开 IME 组合中（避免中文候选按 Esc 误关编辑态）
          if (e.nativeEvent.isComposing || e.nativeEvent.keyCode === 229) return;
          if (e.key === 'Escape') {
            e.stopPropagation();
            setEditing(false);
          }
        }}
      />
      <div className="msg-edit-actions">
        <Button size="small" onClick={() => setEditing(false)}>
          取消
        </Button>
        <Button size="small" type="primary" loading={saving} onClick={onSave}>
          保存
        </Button>
      </div>
    </div>
  );

  if (node.kind === 'user') {
    if (editing) return <div className="msg-user msg-editing">{editBox}</div>;
    return (
      <div className="msg-editable msg-editable-user">
        <UserBubble text={node.text} images={node.images} />
        {!busy ? (
          <button className="msg-edit-btn" title="编辑" onClick={startEdit}>
            <EditOutlined />
          </button>
        ) : null}
      </div>
    );
  }

  if (node.kind === 'assistant') {
    return (
      <div className="msg-editable msg-editable-assistant">
        <AssistantText text={node.text} thinking={node.thinking} streaming={node.streaming} />
      </div>
    );
  }

  switch (node.kind) {
    case 'system':
      return <SystemDivider text={node.text} />;
    case 'tool':
      return (
        <ToolCard
          tool={node.tool}
          arg={node.arg}
          status={node.status}
          outputType={node.outputType}
          output={node.output}
          note={node.note}
          liveOutput={node.liveOutput}
        />
      );
    case 'approval':
      return (
        <ApprovalCard
          callId={node.callId}
          tool={node.tool}
          arg={node.arg}
          reason={node.reason}
          state={node.state}
          onDecide={(callId, decision) => void decide(callId, decision)}
        />
      );
  }
});

interface MessageListProps {
  messages: ChatNode[];
}

/** 消息流：窗口化纯 DOM 渲染 + 跟底状态机 + 分页锚点补偿（机制见文件头注释） */
export function MessageList({ messages }: MessageListProps) {
  const phase = useAgentStore((s) => s.phase);
  const hasMore = useSessionStore((s) => s.hasMore);
  const oldestSeq = useSessionStore((s) => s.oldestSeq);
  const loadingOlder = useSessionStore((s) => s.loadingOlder);
  const loadOlder = useSessionStore((s) => s.loadOlder);

  const listRef = useRef<HTMLDivElement | null>(null);
  const columnRef = useRef<HTMLDivElement | null>(null);

  /** 贴底与否的双份状态：ref 供布局效果/滚动回调同步读，state 驱动置底按钮显隐 */
  const [atBottom, setAtBottom] = useState(true);
  const atBottomRef = useRef(true);
  /** 最近一次主线程写入的 scrollTop 台账：偏离它即判定为用户滚动 */
  const observedTopRef = useRef(0);
  /** 分页锚点：向上分页触发时捕获，pending 期间随用户滚动更新，prepend 落地后消费 */
  const anchorRef = useRef<PagingAnchor | null>(null);
  /** 触顶自动加载的哨兵：置于流顶，进入视口（含提前量）即触发向上分页 */
  const olderSentinelRef = useRef<HTMLDivElement | null>(null);
  const openedRef = useRef(false);
  const lastKeyRef = useRef<string | null>(null);
  const oldestSeqRef = useRef<number | null>(null);
  /** 流尾签名：仅当签名变化（内容增删）才允许跟底，避免贴底态重渲染时把惯性滚动拽到底 */
  const followSigRef = useRef<string | null>(null);

  const lastNode = messages.length > 0 ? messages[messages.length - 1] : undefined;
  const lastKey = lastNode?.id ?? null;
  const lastKind = lastNode?.kind ?? null;
  const followSig = `${hasMore ? 1 : 0}:${oldestSeq ?? ''}:${lastKey}:${messages.length}:${phase ?? ''}`;

  const toBottom = (el: HTMLElement): void => {
    anchorRef.current = null;
    el.scrollTop = el.scrollHeight;
    observedTopRef.current = el.scrollTop;
    atBottomRef.current = true;
    setAtBottom(true);
  };

  /** prepend 落地后按锚点校正 scrollTop（在布局效果中同步执行，浏览器绘制前完成） */
  useLayoutEffect(() => {
    const el = listRef.current;
    if (el === null) return;
    // 首次挂载：直接定位到底部（MessageList 以会话 id 为 key 重挂载，等价 dsh 的 open 跳底）
    if (!openedRef.current) {
      openedRef.current = true;
      toBottom(el);
      oldestSeqRef.current = oldestSeq;
      lastKeyRef.current = lastKey;
      followSigRef.current = followSig;
      return;
    }
    // 窗口头前移（loadOlder prepend）：把锚点行放回原视口位置
    if (
      anchorRef.current !== null &&
      oldestSeq !== null &&
      oldestSeqRef.current !== null &&
      oldestSeq < oldestSeqRef.current
    ) {
      const anchor = anchorRef.current;
      anchorRef.current = null;
      const row = anchorRow(el, anchor.key);
      if (row !== null) el.scrollTop += flowTop(row, el) - anchor.top;
      observedTopRef.current = el.scrollTop;
      oldestSeqRef.current = oldestSeq;
      lastKeyRef.current = lastKey;
      followSigRef.current = followSig;
      return;
    }
    oldestSeqRef.current = oldestSeq;
    // 用户自己发的消息必须可见：检测到尾部新增 user 节点时强制跟底
    const appendedUser = lastKey !== lastKeyRef.current && lastKind === 'user';
    const tipMoved = followSigRef.current !== followSig;
    lastKeyRef.current = lastKey;
    followSigRef.current = followSig;
    // 贴底时跟随新内容；不因贴底态本身的重渲染反复贴底（会打断惯性滚动）
    if (appendedUser || (tipMoved && atBottomRef.current)) toBottom(el);
  });

  const onScrollRef = useRef(() => {});
  onScrollRef.current = () => {
    const el = listRef.current;
    if (el === null) return;
    // 只有用户输入才允许改变贴底归属：程序写入都会同步记录到台账，
    // 偏离台账（>0.5px）即 wheel/touch/滚动条/键盘等用户行为
    const floor = Math.max(0, el.scrollHeight - el.clientHeight);
    const movedByReader = Math.abs(el.scrollTop - Math.min(observedTopRef.current, floor)) > 0.5;
    const isAtBottom = movedByReader
      ? floor - el.scrollTop <= FOLLOW_THRESHOLD + 1
      : atBottomRef.current;
    if (!movedByReader && isAtBottom) {
      toBottom(el);
      return;
    }
    atBottomRef.current = isAtBottom;
    setAtBottom(isAtBottom);
    const position = isAtBottom ? null : scrollPosition(el, el);
    if (isAtBottom) anchorRef.current = null;
    else if (anchorRef.current !== null && position !== null) anchorRef.current = position;
    observedTopRef.current = el.scrollTop;
  };

  // 滚动监听只绑一次（挂在resolved的滚动容器上）
  useEffect(() => {
    const el = listRef.current;
    if (el === null) return;
    const onScroll = (): void => onScrollRef.current();
    el.addEventListener('scroll', onScroll, { passive: true });
    return () => el.removeEventListener('scroll', onScroll);
  }, []);

  /** 贴底时的被动跟随：流式增高、图片加载、输入框伸缩等不触发 scroll 事件，由 RO 兜底 */
  const followRef = useRef<(() => void) | null>(null);
  followRef.current = () => {
    const el = listRef.current;
    if (el !== null && atBottomRef.current) {
      el.scrollTop = el.scrollHeight;
      observedTopRef.current = el.scrollTop;
    }
  };
  useEffect(() => {
    const column = columnRef.current;
    const el = listRef.current;
    if (column === null || el === null || typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(() => {
      followRef.current?.();
    });
    observer.observe(column);
    // 观察滚动容器自身：输入区增高压缩视口（clientHeight 变小）时保持贴底
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  // 分页请求结束（失败/空页）后锚点不再有意义，丢弃
  useEffect(() => {
    if (!loadingOlder) anchorRef.current = null;
  }, [loadingOlder]);

  /** 触发向上分页：先捕获锚点，prepend 落地后由布局效果补偿 */
  const triggerLoadOlder = (): void => {
    const el = listRef.current;
    if (el !== null) {
      const row = pagingAnchor(el, el);
      const key = row?.dataset.chatAnchorKey;
      if (row !== null && key !== undefined) {
        anchorRef.current = { key, top: flowTop(row, el) };
      }
    }
    void loadOlder();
  };
  // 回调经 ref 间接调用，观察器只需在 hasMore/loadingOlder 变化时重建
  const triggerLoadOlderRef = useRef(() => {});
  triggerLoadOlderRef.current = triggerLoadOlder;

  /** 触顶自动加载：哨兵进入视口（rootMargin 提前量）即分页，无需点击按钮 */
  useEffect(() => {
    if (!hasMore) return;
    const sentinel = olderSentinelRef.current;
    if (sentinel === null || typeof IntersectionObserver === 'undefined') return;
    const observer = new IntersectionObserver(
      (entries) => {
        // store 侧有 loadingOlder 重入保护，重复触发是安全的空操作
        if (entries.some((e) => e.isIntersecting)) triggerLoadOlderRef.current();
      },
      // 提前 160px 预取，滚动到顶前内容已就位
      { root: listRef.current, rootMargin: '160px 0px 0px 0px' },
    );
    observer.observe(sentinel);
    return () => observer.disconnect();
  }, [hasMore, loadingOlder]);

  return (
    <div className="chat-root">
      <div ref={listRef} className="chat-list">
        <div ref={columnRef} className="chat-inner" data-chat-flow="">
          {hasMore ? (
            <div className="chat-older" ref={olderSentinelRef}>
              {loadingOlder ? <span className="chat-older-hint">正在加载更早的消息…</span> : null}
            </div>
          ) : null}
          {messages.map((node) => (
            <div
              key={node.id}
              className="flow-item"
              data-chat-anchor-key={node.id}
              data-flow-key={node.id}
            >
              <MessageNode node={node} />
            </div>
          ))}
          {/* 等待首个响应时在流尾渲染「正在思考…」气泡 */}
          {phase === 'thinking' ? <ThinkingBubble /> : null}
        </div>
        {!atBottom ? (
          <div className="to-bottom-slot">
            <button
              type="button"
              className="to-bottom"
              aria-label="回到底部"
              onClick={() => {
                const el = listRef.current;
                if (el !== null) toBottom(el);
              }}
            >
              <ArrowDownOutlined />
            </button>
          </div>
        ) : null}
      </div>
    </div>
  );
}
