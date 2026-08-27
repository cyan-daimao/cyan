import { Virtuoso } from 'react-virtuoso';
import type { ChatNode } from '../../../types';
import { useAgentStore } from '../../../stores/agentStore';
import { UserBubble } from '../../../components/message/UserBubble';
import { AssistantText } from '../../../components/message/AssistantText';
import { SystemDivider } from '../../../components/message/SystemDivider';
import { ToolCard } from '../../../components/message/ToolCard';
import { ApprovalCard } from '../../../components/message/ApprovalCard';

function MessageNode({ node }: { node: ChatNode }) {
  const decide = useAgentStore((s) => s.decide);
  switch (node.kind) {
    case 'user':
      return <UserBubble text={node.text} />;
    case 'assistant':
      return <AssistantText text={node.text} streaming={node.streaming} />;
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
  return (
    <Virtuoso
      data={messages}
      followOutput="auto"
      defaultItemHeight={48}
      itemContent={(_, node) => (
        <div className="chat-inner" style={{ paddingTop: 0, paddingBottom: 0 }}>
          <MessageNode node={node} />
        </div>
      )}
      components={{
        Header: () => <div style={{ height: 24 }} />,
        Footer: () => <div style={{ height: 24 }} />,
      }}
    />
  );
}
