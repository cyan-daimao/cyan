import type { KeyboardEvent, RefObject } from 'react';
import { Segmented, Select } from 'antd';
import { useAgentStore } from '../../../stores/agentStore';
import { useConfigStore } from '../../../stores/configStore';
import { guardBusy, isBusy } from '../../../utils/guard';
import { toast } from '../../../utils/feedback';
import { fmtTokens } from '../../../utils/format';
import type { PermMode } from '../../../types';

interface InputAreaProps {
  draft: string;
  onDraftChange: (v: string) => void;
  inputRef: RefObject<HTMLTextAreaElement>;
}

/** 输入区：权限模式胶囊 + 上下文占用条 + 圆角大输入卡 */
export function InputArea({ draft, onDraftChange, inputRef }: InputAreaProps) {
  const runState = useAgentStore((s) => s.runState);
  const ctxPercent = useAgentStore((s) => s.ctxPercent);
  const tokens = useAgentStore((s) => s.tokens);
  const send = useAgentStore((s) => s.send);
  const interrupt = useAgentStore((s) => s.interrupt);
  const permMode = useConfigStore((s) => s.permMode);
  const setPermMode = useConfigStore((s) => s.setPermMode);
  const models = useConfigStore((s) => s.models);
  const activeModel = useConfigStore((s) => s.activeModel);
  const setActiveModel = useConfigStore((s) => s.setActiveModel);

  const running = runState === 'running' || runState === 'waiting_approval';
  const enabledModels = models.filter((m) => m.status === 'enabled');

  const onSend = async () => {
    const accepted = await send(draft);
    if (accepted) onDraftChange('');
  };

  const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    // Enter 发送、Shift+Enter 换行、Esc 中断
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      if (!isBusy()) void onSend();
    }
    if (e.key === 'Escape' && isBusy()) {
      void interrupt();
    }
  };

  const onPermChange = (v: string | number) => {
    // 运行中禁止切换权限模式（PRD 8.4）
    if (guardBusy('切换权限模式')) return;
    const mode = v as PermMode;
    setPermMode(mode);
    const label = mode === 'plan' ? '计划' : mode === 'ask' ? '询问' : '自动';
    toast.info(`已切换到「${label}」模式`);
  };

  return (
    <div className="input-area">
      {permMode === 'plan' ? (
        <div className="plan-banner">
          🗒️ 计划模式：Agent 只会读代码并给出方案，不会修改文件或执行命令。
        </div>
      ) : null}
      <div className="input-box">
        <div className="input-meta">
          <Segmented
            value={permMode}
            onChange={onPermChange}
            options={[
              { label: '计划', value: 'plan' },
              { label: '询问', value: 'ask' },
              { label: '自动', value: 'auto' },
            ]}
          />
          <div style={{ flex: 1 }} />
          <div className="ctx-meter" title="上下文窗口占用">
            <span className="ctx-label">上下文</span>
            <div className={`ctx-bar${ctxPercent >= 80 ? ' warn' : ''}`}>
              <i style={{ width: `${Math.min(100, ctxPercent)}%` }} />
            </div>
            <span>{ctxPercent}%</span>
          </div>
        </div>
        <div className="input-frame">
          <textarea
            ref={inputRef}
            rows={2}
            placeholder="描述你要完成的任务，例如：帮我修复登录页的超时 bug…"
            value={draft}
            onChange={(e) => onDraftChange(e.target.value)}
            onKeyDown={onKeyDown}
          />
          <div className="input-toolbar">
            <button
              className="icon-btn"
              title="添加附件"
              onClick={() => toast.info('附件上传：当前版本暂未实现')}
            >
              ＋
            </button>
            <span className="input-hint">Enter 发送 · Shift+Enter 换行 · Esc 中断</span>
            <div style={{ flex: 1 }} />
            {tokens.input > 0 || tokens.output > 0 ? (
              <span className="token-line">
                <span>↑ {fmtTokens(tokens.input)}</span>
                <span>↓ {fmtTokens(tokens.output)}</span>
              </span>
            ) : null}
            <Select
              value={activeModel}
              placeholder="选择模型"
              variant="borderless"
              size="small"
              style={{ minWidth: 140 }}
              disabled={running || enabledModels.length === 0}
              onChange={(v: string) => {
                setActiveModel(v);
                toast.info(`已切换模型：${v}`);
              }}
              options={enabledModels.map((m) => ({
                value: m.name,
                label: `${m.name}${m.isDefault ? ' · 默认' : ''}`,
              }))}
            />
            <button
              className={`send-btn${running ? ' stop' : ''}`}
              title={running ? '停止' : '发送'}
              onClick={() => {
                if (running) void interrupt();
                else void onSend();
              }}
            >
              {running ? '■' : '➤'}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
