import { useConfigStore } from '../../../stores/configStore';
import { useProjectStore } from '../../../stores/projectStore';
import type { PermMode } from '../../../types';

const PERM_LABEL: Record<PermMode, string> = { plan: '计划', ask: '询问', auto: '自动' };

/** 推荐项（原型同款） */
const RECOMMENDATIONS = [
  { icon: '🐛', label: '修复 approval 模块的中断 bug', prompt: '帮我修复 approval 模块在中断时 Promise 悬置的 bug' },
  { icon: '⏱️', label: '给 Bash 工具加超时控制', prompt: '给 Bash 工具加上超时控制，默认 2 分钟' },
  { icon: '🔍', label: '解释 agent loop 主循环', prompt: '解释 src/agent/loop.ts 的主循环逻辑' },
  { icon: '📦', label: '初始化为 pnpm monorepo', prompt: '把当前项目初始化为 pnpm monorepo' },
];

interface EmptyStateProps {
  onPick: (text: string) => void;
}

/** 空状态：渐变大标题 + 推荐项 + 环境信息行 */
export function EmptyState({ onPick }: EmptyStateProps) {
  const project = useProjectStore((s) => s.current);
  const permMode = useConfigStore((s) => s.permMode);
  const activeModel = useConfigStore((s) => s.activeModel);

  return (
    <div className="empty-state">
      <div className="greet">有什么我能帮你的吗？</div>
      <div className="rec-wrap">
        <div className="rec-label">为你推荐</div>
        {RECOMMENDATIONS.map((r) => (
          <button key={r.label} className="rec-item" onClick={() => onPick(r.prompt)}>
            {r.icon} {r.label}
          </button>
        ))}
      </div>
      <p className="env-line">
        当前项目：<b className="mono">{project?.path ?? '未打开（点击侧栏「项目」打开）'}</b>
        <br />
        权限模式：<b>{PERM_LABEL[permMode]}</b> · 模型：<b>{activeModel ?? '未配置'}</b>
      </p>
    </div>
  );
}
