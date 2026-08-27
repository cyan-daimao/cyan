/** 展示格式化工具 */

/** token 数 → x.xk */
export function fmtTokens(n: number): string {
  return `${(n / 1000).toFixed(1)}k`;
}

/** 上下文窗口 → 128k */
export function fmtCtxWindow(n: number): string {
  return `${Math.round(n / 1000)}k`;
}
