import type { KeyboardEvent, RefObject } from 'react';
import { useEffect, useMemo, useRef, useState } from 'react';
import { Segmented, Select, Tag, Tooltip } from 'antd';
import {
  ArrowUpOutlined,
  ClearOutlined,
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
import { confirmDanger, toast } from '../../../utils/feedback';
import { fmtTokens } from '../../../utils/format';
import { PermRulesModal } from '../../../components/perms/PermRulesModal';
import type { PermMode, PendingImage, SkillDTO } from '../../../types';

interface InputAreaProps {
  draft: string;
  onDraftChange: (v: string) => void;
  inputRef: RefObject<HTMLTextAreaElement>;
}

/** 图片附件上限（与后端 ServiceError 校验一致） */
const MAX_IMAGES = 4;
/** 单图 base64 长度上限（与后端 MessageImage::MAX_B64_LEN 一致） */
const MAX_IMAGE_B64 = 8_000_000;
/** 允许的图片 MIME（后端白名单子集） */
const IMAGE_MIMES = ['image/png', 'image/jpeg', 'image/webp', 'image/gif'];

let pendingImgSeq = 0;

/** File → PendingImage：读为 base64 并预览；读失败/超限/非法类型返回 null */
function fileToPendingImage(file: File): Promise<PendingImage | null> {
  return new Promise((resolve) => {
    const mime = (file.type || '').toLowerCase() === 'image/jpg' ? 'image/jpeg' : (file.type || '').toLowerCase();
    if (!IMAGE_MIMES.includes(mime)) {
      resolve(null);
      return;
    }
    const reader = new FileReader();
    reader.onerror = () => resolve(null);
    reader.onload = () => {
      const dataUrl = String(reader.result ?? '');
      const data = dataUrl.slice(dataUrl.indexOf(',') + 1);
      if (!data || data.length > MAX_IMAGE_B64) {
        resolve(null);
        return;
      }
      resolve({
        id: `img${(pendingImgSeq += 1)}`,
        mime,
        data,
        dataUrl,
        name: file.name ?? '',
      });
    };
    reader.readAsDataURL(file);
  });
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

  /* ---- IME 防护：记录最近一次 compositionend 时间，onKeyDown 用它放宽 Enter 判定 ---- */
  const lastComposeEndRef = useRef(0);

  /* ---- 图片附件：选图 / 粘贴 / 预览 / 删除 ---- */
  const [images, setImages] = useState<PendingImage[]>([]);
  const fileInputRef = useRef<HTMLInputElement | null>(null);

  /** 批量追加图片（选图 + 拖放 + 粘贴共用）：超出张数/超限的图片跳过并提示 */
  const addImages = async (files: File[]) => {
    const candidates = files.filter((f) => f.type.startsWith('image/'));
    if (candidates.length === 0) return;
    let remaining = MAX_IMAGES - images.length;
    if (remaining <= 0) {
      toast.warning(`最多上传 ${MAX_IMAGES} 张图片`);
      return;
    }
    const added: PendingImage[] = [];
    for (const file of candidates) {
      if (added.length >= remaining) {
        toast.warning(`最多上传 ${MAX_IMAGES} 张图片，多余图片已忽略`);
        break;
      }
      const img = await fileToPendingImage(file);
      if (img === null) {
        toast.warning(`图片不支持或超过 6MB：${file.name || '粘贴的图片'}`);
        continue;
      }
      added.push(img);
    }
    if (added.length > 0) setImages((prev) => [...prev, ...added].slice(0, MAX_IMAGES));
  };

  const removeImage = (id: string) => {
    setImages((prev) => prev.filter((img) => img.id !== id));
  };

  const pickImages = () => {
    if (images.length >= MAX_IMAGES) {
      toast.warning(`最多上传 ${MAX_IMAGES} 张图片`);
      return;
    }
    fileInputRef.current?.click();
  };

  const onFileInputChange = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const files = Array.from(e.target.files ?? []);
    e.target.value = '';
    await addImages(files);
  };

  /* ---- `/` 技能补全（PLUGIN_DESIGN 2.3） ---- */
  const project = useProjectStore((s) => s.current);
  const skills = useSkillStore((s) => s.skills);
  const loadSkills = useSkillStore((s) => s.load);
  const clearSessionInStore = useSessionStore((s) => s.clearSession);
  const [activeIdx, setActiveIdx] = useState(0);
  const [suggestClosed, setSuggestClosed] = useState(false);

  // 项目切换时加载技能（无项目传空串，只列全局）
  useEffect(() => {
    void loadSkills(project?.path ?? '');
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [project?.path]);

  /** /clear 内置命令（非磁盘技能） */
  const CLEAR_SKILL: SkillDTO = {
    id: 'clear',
    name: '清空上下文',
    description: '删除本会话全部消息与工具记录，从零开始（不可恢复）',
    enabled: true,
    source: 'global',
    pluginName: null,
    marketRepo: null,
    content: '',
  };

  /** 补全候选：内置 /clear + 按 id/名称/描述过滤已启用技能 */
  const matches = useMemo<SkillDTO[]>(() => {
    if (!draft.startsWith('/')) return [];
    const kw = draft.slice(1).toLowerCase();
    if (kw.includes('\n')) return [];
    const builtin = 'clear'.startsWith(kw) ? [CLEAR_SKILL] : [];
    const filtered = skills.filter(
      (s) =>
        s.enabled &&
        (!kw ||
          s.id.toLowerCase().includes(kw) ||
          s.name.toLowerCase().includes(kw) ||
          s.description.toLowerCase().includes(kw)),
    );
    return [...builtin, ...filtered];
  }, [draft, skills]);

  const suggestOpen = matches.length > 0 && !suggestClosed;

  /** 选中候选：/clear 直接执行清空（二次确认），其余技能填入 /id 标记 */
  const pickSkill = (s: SkillDTO) => {
    if (s.id === 'clear') {
      setSuggestClosed(true);
      doClear();
      return;
    }
    onDraftChangeWrap(`/${s.id} `);
    setTimeout(() => inputRef.current?.focus(), 0);
  };

  /** 执行清空上下文：需有激活会话；antd Modal 二次确认防误触（PRD 第 9 章） */
  const doClear = () => {
    const sid = useSessionStore.getState().activeId;
    if (sid === null) {
      toast.warning('请先开始一个对话');
      return;
    }
    confirmDanger({
      title: '清空上下文',
      content: '将删除本会话的全部消息与工具记录，操作不可恢复。',
      okText: '清空',
      onOk: async () => {
        const removed = await clearSessionInStore(sid);
        if (removed !== null) {
          toast.success(`已清空上下文（删除 ${removed} 条消息）`);
          setTimeout(() => inputRef.current?.focus(), 0);
        }
      },
    });
  };

  /** 统一的草稿更新：重置补全状态 */
  const onDraftChangeWrap = (v: string) => {
    onDraftChange(v);
    setSuggestClosed(false);
    setActiveIdx(0);
  };

  /**
   * 发送前展开技能引用：`/id 参数` → 技能正文（$ARGUMENTS 替换为参数）。
   * 未匹配到技能则原样发送（安全降级）。
   */
  const expandSkillRef = (raw: string): string => {
    const m = raw.match(/^\/([a-zA-Z0-9_-]+)(?:\s+([\s\S]*))?$/);
    if (!m) return raw;
    const skill = skills.find((s) => s.id === m[1] && s.enabled);
    if (!skill) return raw;
    const args = (m[2] ?? '').trim();
    return skill.content.replace(/\$ARGUMENTS/g, args);
  };

  const onSend = async () => {
    // /clear 命令：清空上下文而非发送
    if (draft.trim() === '/clear' && images.length === 0) {
      onDraftChangeWrap('');
      doClear();
      return;
    }
    // 文本与图片至少其一；发图时允许空文本（后端提示词会补齐）
    if (!draft.trim() && images.length > 0) {
      toast.warning('请补充图片相关的任务描述');
      return;
    }
    const expanded = expandSkillRef(draft.trim());
    const accepted = await send(expanded, { images });
    if (accepted) {
      onDraftChangeWrap('');
      setImages([]);
    }
  };

  const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    // 输入法组合中（拼音候选未上屏）：按键交给 IME，不触发发送/补全/中断
    if (e.nativeEvent.isComposing || e.nativeEvent.keyCode === 229) return;
    // 中文 IME 刚上屏的下一帧（compositionend 后的几十毫秒），WebKit 仍可能把后续
    // 按键当成"句首"并自动大写或插入多余空格；这段时间内 Enter 只换行不发送。
    if (Date.now() - lastComposeEndRef.current < 200 && e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      return;
    }
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

  /** 粘贴图片：把剪贴板中的图片文件加入待传列表（文本粘贴走默认行为） */
  const onPaste = (e: React.ClipboardEvent<HTMLTextAreaElement>) => {
    const files = Array.from(e.clipboardData?.files ?? []).filter((f) => f.type.startsWith('image/'));
    if (files.length === 0) return;
    e.preventDefault();
    void addImages(files);
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
          <Tooltip title="当前会话的权限规则">
            <button
              className="icon-btn"
              onClick={() => {
                if (activeId === null) {
                  toast.warning('请先开始一个对话');
                  return;
                }
                setPermsOpen(true);
              }}
            >
              <SafetyOutlined />
            </button>
          </Tooltip>
          <Tooltip title="清空上下文（/clear）">
            <button
              className="icon-btn"
              title="清空本会话全部上下文"
              disabled={activeId === null || sessionBusy}
              onClick={doClear}
            >
              <ClearOutlined />
            </button>
          </Tooltip>
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
            // 中文 IME 下 WebKit 会自动首字母大写、插入自动空格；显式关掉
            autoCapitalize="off"
            autoCorrect="off"
            spellCheck={false}
            onChange={(e) => onDraftChangeWrap(e.target.value)}
            onKeyDown={onKeyDown}
            onPaste={onPaste}
            onCompositionEnd={(e) => {
              lastComposeEndRef.current = Date.now();
              // compositionend 之后再 normalize 一次，防止拼音候选上屏后残留首字母大写/多余空格
              const next = (e.currentTarget.value || '').replace(/^\s+|\s+$/g, '');
              if (next !== e.currentTarget.value) onDraftChangeWrap(next);
            }}
          />
          {/* 待发送图片预览条 */}
          {images.length > 0 ? (
            <div className="img-previews">
              {images.map((img) => (
                <div key={img.id} className="img-preview" title={img.name || '粘贴的图片'}>
                  <img src={img.dataUrl} alt={img.name || '附件图片'} />
                  <button
                    className="img-remove"
                    title="移除"
                    onClick={() => removeImage(img.id)}
                  >
                    ×
                  </button>
                </div>
              ))}
            </div>
          ) : null}
          <div className="input-toolbar">
            <input
              ref={fileInputRef}
              type="file"
              accept="image/png,image/jpeg,image/webp,image/gif"
              multiple
              hidden
              onChange={(e) => void onFileInputChange(e)}
            />
            <Tooltip title={`上传图片（最多 ${MAX_IMAGES} 张）`}>
              <button
                className="icon-btn"
                title="添加图片"
                disabled={sessionBusy}
                onClick={pickImages}
              >
                <PlusOutlined />
              </button>
            </Tooltip>
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
