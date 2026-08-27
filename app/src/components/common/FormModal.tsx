import { Modal } from 'antd';
import type { ReactNode } from 'react';

interface FormModalProps {
  open: boolean;
  title: string;
  confirmLoading?: boolean;
  width?: number;
  onCancel: () => void;
  onOk: () => void;
  children: ReactNode;
}

/** 通用表单弹窗壳：模型 / MCP / 权限规则表单复用（TECH_DESIGN 2.6） */
export function FormModal({
  open,
  title,
  confirmLoading,
  width = 520,
  onCancel,
  onOk,
  children,
}: FormModalProps) {
  return (
    <Modal
      open={open}
      title={title}
      width={width}
      okText="保存"
      cancelText="取消"
      confirmLoading={confirmLoading}
      onCancel={onCancel}
      onOk={onOk}
      destroyOnClose
      maskClosable={false}
    >
      {children}
    </Modal>
  );
}
