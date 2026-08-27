/** 会话列表 emoji 头像分配：按标题哈希稳定取值 */

const EMOJIS = ['🐛', '⏱️', '🔍', '📦', '🛠️', '✨', '📄', '🧪', '🚀', '🎨', '🔧', '💡'];

export function sessionEmoji(title: string): string {
  let h = 0;
  for (let i = 0; i < title.length; i += 1) {
    h = (h * 31 + title.charCodeAt(i)) >>> 0;
  }
  return EMOJIS[h % EMOJIS.length];
}
