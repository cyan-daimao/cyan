/** 展示格式化工具 */

/** token 数 → 980 / 12.3k / 1.5M（自适应单位，不足 1k 原样） */
export function fmtTokens(n: number): string {
  if (n < 1000) return `${n}`;
  if (n < 1_000_000) return `${(n / 1000).toFixed(1)}k`;
  return `${(n / 1_000_000).toFixed(1)}M`;
}

/** 上下文窗口 → 128k / 1M / 2M（整数化，1M 及以上用 M） */
export function fmtCtxWindow(n: number): string {
  if (n < 1_000_000) return `${Math.round(n / 1000)}k`;
  return `${Math.round(n / 1_000_000)}M`;
}
