import { App as AntdApp } from 'antd';
import type { ReactNode } from 'react';

/**
 * 全局反馈工具：在 antd <App> 上下文内绑定 message / modal 实例，
 * 使 store、service 等非组件环境也能弹出 Toast 与二次确认。
 */

type MessageApi = ReturnType<typeof AntdApp.useApp>['message'];
type ModalApi = ReturnType<typeof AntdApp.useApp>['modal'];

let messageApi: MessageApi | null = null;
let modalApi: ModalApi | null = null;

/** 挂载在 <AntdApp> 内，完成 message / modal 实例绑定 */
export function FeedbackBinder() {
  const { message, modal } = AntdApp.useApp();
  messageApi = message;
  modalApi = modal;
  return null;
}

/** 顶部胶囊 Toast（PRD 第 9 章） */
export const toast = {
  success: (content: string) => void messageApi?.success(content),
  error: (content: string) => void messageApi?.error(content),
  warning: (content: string) => void messageApi?.warning(content),
  info: (content: string) => void messageApi?.info(content),
};

/** 危险操作二次确认（PRD 第 9 章：说明后果，红色确认键） */
export function confirmDanger(opts: {
  title: string;
  content: ReactNode;
  okText?: string;
  onOk: () => void | Promise<void>;
}) {
  modalApi?.confirm({
    title: opts.title,
    content: opts.content,
    okText: opts.okText ?? '确认',
    cancelText: '取消',
    okButtonProps: { danger: true },
    onOk: opts.onOk,
  });
}

/** 提取错误的可读信息 */
export function errText(e: unknown): string {
  if (e instanceof Error) return e.message;
  return String(e);
}
