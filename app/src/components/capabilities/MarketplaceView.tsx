import { useEffect, useMemo, useState } from 'react';
import { Button, Input, Segmented, Spin } from 'antd';
import { DownloadOutlined, StarOutlined } from '@ant-design/icons';
import type { MarketItemDTO, MarketSource } from '../../types';
import { installPluginFromGithub, searchMarketplace } from '../../services/plugin';
import { installSkillFromGithub, searchSkillMarket } from '../../services/skill';
import { usePluginStore } from '../../stores/pluginStore';
import { useSkillStore } from '../../stores/skillStore';
import { useProjectStore } from '../../stores/projectStore';
import { errText, toast } from '../../utils/feedback';
import { openExternal } from '../../utils/openExternal';
import { Empty } from '../common/Empty';

/** 从搜索词解析可直接安装的仓库 fullName（github.com/owner/repo 或 owner/repo） */
function parseDirectRepo(kw: string): string | null {
  const m = kw.match(/(?:github\.com\/)?([a-zA-Z0-9_.-]+\/[a-zA-Z0-9_.-]+)/);
  if (!m) return null;
  const full = m[1].replace(/\/+$/, '').replace(/\.git$/, '');
  // owner/repo 两段都不能为空
  const [owner, repo] = full.split('/');
  return owner && repo ? `${owner}/${repo}` : null;
}

/** 解析 Gitee 仓库地址（gitee.com/owner/repo 或 owner/repo，去掉 .git 后缀） */
function parseGiteeRepo(kw: string): string | null {
  const m = kw.trim().match(/(?:gitee\.com\/)?([a-zA-Z0-9_.-]+\/[a-zA-Z0-9_.-]+)/);
  if (!m) return null;
  const full = m[1].replace(/\/+$/, '').replace(/\.git$/, '');
  const [owner, repo] = full.split('/');
  return owner && repo ? `${owner}/${repo}` : null;
}

/** 市场维度（插件 / 技能）差异化配置 */
interface MarketDim {
  search: (keyword: string, source: MarketSource) => Promise<MarketItemDTO[]>;
  /** 安装（内部处理 toast 与对应 store 刷新） */
  install: (fullName: string, source: MarketSource) => Promise<void>;
  emptyText: string;
  isInstalled: (fullName: string) => boolean;
}

/** 源切换选项 */
const SOURCE_OPTIONS = [
  { label: 'GitHub', value: 'github' },
  { label: 'Gitee（国内）', value: 'gitee' },
];

/** 市场搜索面板：插件与技能两个维度共用（各自持有独立实例状态） */
function MarketSearchPanel({ dim }: { dim: MarketDim }) {
  const [source, setSource] = useState<MarketSource>('github');
  const [keyword, setKeyword] = useState('');
  const [items, setItems] = useState<MarketItemDTO[]>([]);
  const [loading, setLoading] = useState(false);
  const [searched, setSearched] = useState(false);
  /** 安装中的 fullName */
  const [installing, setInstalling] = useState<string | null>(null);

  const search = async (kw: string, src: MarketSource) => {
    setLoading(true);
    try {
      setItems(await dim.search(kw, src));
      setSearched(true);
    } catch (e) {
      toast.error(`搜索市场失败：${errText(e)}`);
    } finally {
      setLoading(false);
    }
  };

  // 初始进入用空 keyword 加载推荐列表（GitHub 源）
  useEffect(() => {
    void search('', 'github');
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 切源：GitHub 重新拉推荐列表；Gitee 清空列表（不支持浏览，走直达安装）
  const onSourceChange = (src: MarketSource) => {
    setSource(src);
    setItems([]);
    setSearched(false);
    if (src === 'github') void search(keyword, 'github');
  };

  const onInstall = async (fullName: string, src: MarketSource) => {
    setInstalling(fullName);
    try {
      await dim.install(fullName, src);
    } catch (e) {
      toast.error(`安装失败：${errText(e)}`);
    } finally {
      setInstalling(null);
    }
  };

  // Gitee 源：输入词形如 owner/repo 时展示直达安装项（详情在搜索时拉取）
  const giteeDirect = useMemo(() => {
    if (source !== 'gitee') return null;
    return parseGiteeRepo(keyword);
  }, [source, keyword]);

  // GitHub 源：搜索词形如仓库地址 / owner/repo 时，结果区顶部提供直接安装项
  const directRepo = useMemo(() => {
    const kw = keyword.trim();
    if (source !== 'github' || !kw) return null;
    const repo = parseDirectRepo(kw);
    if (!repo) return null;
    // 与搜索结果重复时不重复展示
    return items.some((it) => it.fullName.toLowerCase() === repo.toLowerCase()) ? null : repo;
  }, [source, keyword, items]);

  return (
    <div>
      <div style={{ display: 'flex', gap: 12, marginBottom: 12, alignItems: 'center' }}>
        <Segmented
          options={SOURCE_OPTIONS}
          value={source}
          onChange={(v) => onSourceChange(v as MarketSource)}
        />
        <span style={{ color: 'var(--text-tertiary, #999)', fontSize: 12, flex: 1 }}>
          {source === 'gitee'
            ? 'Gitee 源：输入 owner/repo 仓库地址直接安装'
            : 'GitHub 源：按关键词搜索插件仓库'}
        </span>
      </div>
      <Input.Search
        placeholder={
          source === 'gitee' ? '例如：openharmony-sig/docs（owner/repo）' : '搜索插件仓库…'
        }
        allowClear
        value={keyword}
        loading={loading}
        autoCapitalize="off"
        autoCorrect="off"
        spellCheck={false}
        onChange={(e) => setKeyword(e.target.value)}
        onSearch={(v) => {
          if (source === 'gitee') {
            // Gitee：输入 owner/repo 即拉详情作直达安装项
            const repo = parseGiteeRepo(v);
            if (!repo) {
              toast.warning('请输入 owner/repo 形式的仓库地址');
              return;
            }
            setItems([]);
            setSearched(false);
            void search(repo, 'gitee');
          } else {
            void search(v.trim(), 'github');
          }
        }}
        style={{ marginBottom: 12 }}
      />
      <Spin spinning={loading && !searched}>
        <div className="market-list">
          {source === 'gitee' && giteeDirect && !searched ? (
            <div className="market-card market-direct">
              <div className="mc-main">
                <div className="mc-name mono">{giteeDirect}</div>
                <div className="mc-desc">Gitee 仓库直达安装</div>
              </div>
              <Button
                type="primary"
                icon={<DownloadOutlined />}
                loading={installing === giteeDirect}
                onClick={() => void onInstall(giteeDirect, 'gitee')}
              >
                直接安装
              </Button>
            </div>
          ) : null}
          {directRepo ? (
            <div className="market-card market-direct">
              <div className="mc-main">
                <div className="mc-name mono">{directRepo}</div>
                <div className="mc-desc">直接安装该 GitHub 仓库</div>
              </div>
              <Button
                type="primary"
                icon={<DownloadOutlined />}
                loading={installing === directRepo}
                disabled={dim.isInstalled(directRepo)}
                onClick={() => void onInstall(directRepo, 'github')}
              >
                {dim.isInstalled(directRepo) ? '已安装' : '直接安装'}
              </Button>
            </div>
          ) : null}
          {items.length === 0 && !loading && searched ? (
            <Empty text={source === 'gitee' ? '未找到该 Gitee 仓库，请检查地址' : dim.emptyText} />
          ) : (
            items.map((it) => {
              const done = dim.isInstalled(it.fullName);
              return (
                <div className="market-card" key={`${source}:${it.fullName}`}>
                  <div className="mc-main">
                    <div className="mc-head">
                      <a
                        className="mc-name mono"
                        href={it.url}
                        onClick={(e) => {
                          e.preventDefault();
                          void openExternal(it.url);
                        }}
                      >
                        {it.fullName}
                      </a>
                      <span className="mc-author">{it.author}</span>
                      <span className="mc-stars">
                        <StarOutlined /> {it.stars}
                      </span>
                    </div>
                    <div className="mc-desc" title={it.description ?? ''}>
                      {it.description ?? '（无描述）'}
                    </div>
                  </div>
                  <Button
                    type={done ? 'default' : 'primary'}
                    icon={<DownloadOutlined />}
                    loading={installing === it.fullName}
                    disabled={done}
                    onClick={() => void onInstall(it.fullName, source)}
                  >
                    {done ? '已安装' : '安装'}
                  </Button>
                </div>
              );
            })
          )}
        </div>
      </Spin>
    </div>
  );
}

/** 插件市场视图（挂在能力面板「插件」Tab 的市场分支） */
export function PluginMarketView() {
  const installedPlugins = usePluginStore((s) => s.plugins);
  const reloadPlugins = usePluginStore((s) => s.load);

  const dim = useMemo<MarketDim>(
    () => ({
      search: searchMarketplace,
      install: async (fullName, source) => {
        const dto = await installPluginFromGithub(fullName, source);
        await reloadPlugins(true);
        toast.success(
          `已安装 ${dto.name} v${dto.version}（技能 ${dto.skillCount} · MCP ${dto.mcpCount} · 规则 ${dto.ruleCount}）`,
        );
      },
      emptyText: '没有匹配的插件仓库（约定：cyan 插件仓库需打 cyan-plugin topic）',
      // 已安装判定：插件名 = 仓库 repo 段
      isInstalled: (fullName) =>
        installedPlugins.some((p) => p.name === (fullName.split('/')[1] ?? '')),
    }),
    [installedPlugins, reloadPlugins],
  );

  return <MarketSearchPanel dim={dim} />;
}

/** 技能市场视图（挂在能力面板「技能」Tab 的市场分支） */
export function SkillMarketView() {
  const skills = useSkillStore((s) => s.skills);
  const loadSkills = useSkillStore((s) => s.load);
  const skillLoadedFor = useSkillStore((s) => s.loadedFor);
  const project = useProjectStore((s) => s.current);

  // 进入市场时确保技能 store 已加载（按项目缓存键去重）
  useEffect(() => {
    void loadSkills(project?.path ?? '');
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [project?.path]);

  const dim = useMemo<MarketDim>(
    () => ({
      search: searchSkillMarket,
      install: async (fullName, source) => {
        const list = await installSkillFromGithub(fullName, source);
        // 强制刷新技能列表，切回「已安装」立即可见（来源带「市场」Tag）
        await loadSkills(skillLoadedFor ?? project?.path ?? '', true);
        toast.success(`已安装 ${list.length} 个技能：${list.map((s) => s.name).join('、')}`);
      },
      emptyText: '没有匹配的技能仓库（约定：cyan 技能仓库需打 cyan-skill topic）',
      // 已安装判定：存在 marketRepo 对应该仓库的技能
      isInstalled: (fullName) => skills.some((s) => s.marketRepo === fullName),
    }),
    [skills, loadSkills, skillLoadedFor, project?.path],
  );

  return <MarketSearchPanel dim={dim} />;
}
