import { useState } from 'react';
import { Virtuoso } from 'react-virtuoso';
import { Button } from 'antd';
import { EditOutlined } from '@ant-design/icons';
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

interface MessageNodeProps {
  node: ChatNode;
  index: number;
  total: number;
}

/**
 * 消息节点。仅 user 文本消息支持行内编辑（编辑即截断重发）：
 * 编辑第 i 条会物理删除其后所有消息，并自动以新文本重发。
 */
function MessageNode({ node, index, total }: MessageNodeProps) {
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
    const trailing = total - index - 1;
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
        <UserBubble text={node.text} />
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
}

interface MessageListProps {
  messages: ChatNode[];
}

/** 消息流：react-virtuoso 虚拟列表（长会话不卡），自动跟随底部 */
export function MessageList({ messages }: MessageListProps) {
  const phase = useAgentStore((s) => s.phase);
  return (
    <Virtuoso
      data={messages}
      // 打开/切换会话时直接定位到末尾（父组件以会话 id 作为 key 触发重挂载）
      initialTopMostItemIndex={Math.max(messages.length - 1, 0)}
      followOutput="auto"
      defaultItemHeight={48}
      computeItemKey={(_, node) => node.id}
      itemContent={(index, node) => (
        <div className="chat-inner" style={{ paddingTop: 0, paddingBottom: 0 }}>
          <MessageNode node={node} index={index} total={messages.length} />
        </div>
      )}
      components={{
        Header: () => <div style={{ height: 24 }} />,
        // 发送后到首个响应之间在消息流末尾渲染「正在思考…」气泡
        Footer: () => (
          <div className="chat-inner" style={{ paddingTop: 0, paddingBottom: 0 }}>
            {phase === 'thinking' ? <ThinkingBubble /> : null}
            <div style={{ height: 24 }} />
          </div>
        ),
      }}
    />
  );
}
