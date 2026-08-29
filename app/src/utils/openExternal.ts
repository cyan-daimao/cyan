import { openUrl } from '@tauri-apps/plugin-opener';
import { toast } from './feedback';

/**
 * 打开外部链接（Tauri WebView 内 target=_blank 无效，必须走 opener 插件）。
 * 失败时降级 window.open（纯浏览器调试环境兜底），再不行 toast。
 */
export async function openExternal(url: string): Promise<void> {
  try {
    await openUrl(url);
    return;
  } catch {
    // 非 Tauri 环境或 opener 不可用：降级
  }
  const win = window.open(url, '_blank', 'noopener,noreferrer');
  if (!win) toast.error('无法打开链接，请检查浏览器设置');
}
