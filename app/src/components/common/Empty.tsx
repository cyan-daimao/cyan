import { Empty as AntdEmpty } from 'antd';

/** 统一空态：表格 / 列表空数据展示（PRD 第 9 章） */
export function Empty({ text }: { text: string }) {
  return <AntdEmpty image={AntdEmpty.PRESENTED_IMAGE_SIMPLE} description={text} />;
}
