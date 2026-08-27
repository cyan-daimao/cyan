import { useEffect, useState } from 'react';

/** 监听窗口宽度（响应式：<1100px 隐藏文件面板，<860px 侧栏默认收起） */
export function useWindowWidth(): number {
  const [width, setWidth] = useState(() => window.innerWidth);
  useEffect(() => {
    const onResize = () => setWidth(window.innerWidth);
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  }, []);
  return width;
}
