import type { KeyboardEvent, RefObject } from 'react';
import { useEffect, useMemo, useState } from 'react';
import { Segmented, Select, Tag, Tooltip } from 'antd';
import {
  ArrowUpOutlined,
  LoadingOutlined,
  PlusOutlined,
  ReadOutlined,
  SafetyOutlined,
  StopOutlined,
} from '@ant-design/icons';
import { useAgentStore } from '../../../stores/agentStore';
import { useConfigStore } from '../../../stores/configStore';
import { useSessionStore } from '../../../stores/sessionStore';
import { useSkillStore } from '../../../stores/skillStore';
import { useProjectStore } from '../../../stores/projectStore';
import { guardBusy, isBusy } from '../../../utils/guard';
import { toast } from '../../../utils/feedback';
import { fmtTokens } from '../../../utils/format';
import { PermRulesModal } from '../../../components/perms/PermRulesModal';
import type { PermMode, SkillDTO } from '../../../types';

interface InputAreaProps {
  draft: string;
  onDraftChange: (v: string) => void;
  inputRef: RefObject<HTMLTextAreaElement>;
}

/** 输入区：权限模式胶囊 + 上下文占用条 + 圆角大输入卡 + `/` 技能补全 */
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
  const sessionModels = useConfigStore((s) => s.sessionModels);
  const setSessionModel = useConfigStore((s) => s.setSessionModel);

  const running = runState === 'running' || runState === 'waiting_approval';
  const enabledModels = models.filter((m) => m.status === 'enabled');
  const activeId = useSessionStore((s) => s.activeId);
  /** 当前会话运行中（含等待审批）：模型下拉禁用 */
  const sessionBusy = useAgentStore((s) => {
    const f = activeId === null ? undefined : s.sessionRuns[activeId];
    return f === 'running' || f === 'waiting_approval';
  });
  /** 当前会话运行开始时间（无则 undefined） */
  const startedAt = useAgentStore((s) =>
    activeId === null ? undefined : s.runStartedAt[activeId],
  );

  /* ---- 运行计时：每秒刷新，停止/卸载时清理 ---- */
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!sessionBusy || startedAt === undefined) return;
    setNow(Date.now());
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, [sessionBusy, startedAt]);

  /** 已执行时长 mm:ss（不足 1 分钟如 0:07） */
  const elapsed = (() => {
    if (startedAt === undefined) return '0:00';
    const sec = Math.max(0, Math.floor((now - startedAt) / 1000));
    return `${Math.floor(sec / 60)}:${String(sec % 60).padStart(2, '0')}`;
  })();
  /** 下拉显示值：会话级偏好 → 全局选中 → 默认模型 */
  const displayModel =
    (activeId !== null ? sessionModels[activeId] : undefined) ??
    activeModel ??
    models.find((m) => m.isDefault && m.status === 'enabled')?.name ??
    null;
  const [permsOpen, setPermsOpen] = useState(false);

  /* ---- `/` 技能补全（PLUGIN_DESIGN 2.3） ---- */
  const project = useProjectStore((s) => s.current);
  const skills = useSkillStore((s) => s.skills);
  const loadSkills = useSkillStore((s) => s.load);
  const [activeIdx, setActiveIdx] = useState(0);
  const [suggestClosed, setSuggestClosed] = useState(false);

  // 项目切换时加载技能（无项目传空串，只列全局）
  useEffect(() => {
    void loadSkills(project?.path ?? '');
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [project?.path]);

  /** 补全候选：草稿以 / 开头时，按 id/名称/描述过滤已启用技能 */
  const matches = useMemo<SkillDTO[]>(() => {
    if (!draft.startsWith('/')) return [];
    const kw = draft.slice(1).toLowerCase();
    if (kw.includes('\n')) return [];
    return skills.filter(
      (s) =>
        s.enabled &&
        (!kw ||
          s.id.toLowerCase().includes(kw) ||
          s.name.toLowerCase().includes(kw) ||
          s.description.toLowerCase().includes(kw)),
    );
  }, [draft, skills]);

  const suggestOpen = matches.length > 0 && !suggestClosed;

  /** 选中技能：草稿替换为技能正文（$ARGUMENTS 原样保留），聚焦输入框 */
  const pickSkill = (s: SkillDTO) => {
    onDraftChange(s.content);
    setSuggestClosed(true);
    setTimeout(() => inputRef.current?.focus(), 0);
  };

  const onDraftChangeWrap = (v: string) => {
    onDraftChange(v);
    setSuggestClosed(false);
    setActiveIdx(0);
  };

  const onSend = async () => {
    const accepted = await send(draft);
    if (accepted) onDraftChangeWrap('');
  };

  const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    // 输入法组合中（拼音候选未上屏）：按键交给 IME，不触发发送/补全/中断
    if (e.nativeEvent.isComposing || e.nativeEvent.keyCode === 229) return;
    // 技能补全打开时：上下键移动、Enter 选中、Esc 关闭（不触发发送/中断）
    if (suggestOpen) {
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        setActiveIdx((i) => (i + 1) % matches.length);
        return;
      }
      if (e.key === 'ArrowUp') {
        e.preventDefault();
        setActiveIdx((i) => (i - 1 + matches.length) % matches.length);
        return;
      }
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        pickSkill(matches[activeIdx] ?? matches[0]);
        return;
      }
      if (e.key === 'Escape') {
        e.preventDefault();
        setSuggestClosed(true);
        return;
      }
    }
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
          <ReadOutlined /> 计划模式：Agent 只会读代码并给出方案，不会修改文件或执行命令。
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
          <button
            className="icon-btn"
            title="当前会话的权限规则"
            onClick={() => {
              if (activeId === null) {
                toast.warning('请先开始一个对话');
                return;
              }
              setPermsOpen(true);
            }}
          >
            <SafetyOutlined /> 规则
          </button>
          <div style={{ flex: 1 }} />
          {sessionBusy && startedAt !== undefined ? (
            <span className="run-timer" title="已执行时长">
              <LoadingOutlined /> {elapsed}
            </span>
          ) : null}
          <div className="ctx-meter" title="上下文窗口占用">
            <span className="ctx-label">上下文</span>
            <div className={`ctx-bar${ctxPercent >= 80 ? ' warn' : ''}`}>
              <i style={{ width: `${Math.min(100, ctxPercent)}%` }} />
            </div>
            <span>{ctxPercent}%</span>
          </div>
        </div>
        <div className="input-frame">
          {suggestOpen ? (
            <div className="skill-suggest">
              {matches.map((s, i) => (
                <div
                  key={`${s.source}:${s.id}`}
                  className={`skill-suggest-item${i === activeIdx ? ' active' : ''}`}
                  // mousedown 抢先于 blur，保持输入框焦点
                  onMouseDown={(e) => {
                    e.preventDefault();
                    pickSkill(s);
                  }}
                  onMouseEnter={() => setActiveIdx(i)}
                >
                  <span className="ss-name mono">/{s.id}</span>
                  <span className="ss-label">{s.name}</span>
                  <Tag
                    className="ss-tag"
                    color={s.source === 'project' ? 'processing' : s.source === 'plugin' ? 'purple' : undefined}
                  >
                    {s.source === 'project' ? '项目' : s.source === 'plugin' ? '插件' : '全局'}
                  </Tag>
                  <span className="ss-desc" title={s.description}>
                    {s.description}
                  </span>
                </div>
              ))}
            </div>
          ) : null}
          <textarea
            ref={inputRef}
            rows={2}
            placeholder="描述你要完成的任务，例如：帮我修复登录页的超时 bug…"
            value={draft}
            onChange={(e) => onDraftChangeWrap(e.target.value)}
            onKeyDown={onKeyDown}
          />
          <div className="input-toolbar">
            <button
              className="icon-btn"
              title="添加附件"
              onClick={() => toast.info('附件上传：当前版本暂未实现')}
            >
              <PlusOutlined />
            </button>
            <span className="input-hint">Enter 发送 · Shift+Enter 换行 · Esc 中断 · / 技能</span>
            <div style={{ flex: 1 }} />
            {tokens.input > 0 || tokens.output > 0 ? (
              <span className="token-line">
                <span>↑ {fmtTokens(tokens.input)}</span>
                <span>↓ {fmtTokens(tokens.output)}</span>
              </span>
            ) : null}
            <Tooltip title={sessionBusy || running ? '运行中不可切换模型' : undefined}>
              <Select
                value={displayModel}
                placeholder="选择模型"
                variant="borderless"
                size="small"
                style={{ minWidth: 140 }}
                disabled={running || sessionBusy || enabledModels.length === 0}
                onChange={(v: string) => {
                  // 有激活会话 → 写会话级偏好；空会话 → 维持原逻辑写全局
                  if (activeId !== null) {
                    void setSessionModel(activeId, v).then((ok) => {
                      if (ok) toast.info(`当前会话模型：${v}`);
                    });
                  } else {
                    setActiveModel(v);
                    toast.info(`已切换模型：${v}`);
                  }
                }}
                options={enabledModels.map((m) => ({
                  value: m.name,
                  label: `${m.name}${m.isDefault ? ' · 默认' : ''}`,
                }))}
              />
            </Tooltip>
            <button
              className={`send-btn${running ? ' stop' : ''}`}
              title={running ? '停止' : '发送'}
              onClick={() => {
                if (running) void interrupt();
                else void onSend();
              }}
            >
              {running ? <StopOutlined /> : <ArrowUpOutlined />}
            </button>
          </div>
        </div>
      </div>
      <PermRulesModal open={permsOpen} onClose={() => setPermsOpen(false)} />
    </div>
  );
}
