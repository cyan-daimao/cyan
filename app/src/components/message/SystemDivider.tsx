/** 系统消息分割线（中断 / 完成 / 切换项目 / 压缩提示） */
export function SystemDivider({ text }: { text: string }) {
  return (
    <div className="msg-system">
      <span>{text}</span>
    </div>
  );
}
