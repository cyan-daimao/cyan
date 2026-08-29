import { useEffect, useMemo, useState } from 'react';
import { Button, Input, Spin, Tag, Tooltip } from 'antd';
import { DownloadOutlined, LinkOutlined } from '@ant-design/icons';
import type { McpMarketItemDTO } from '../../types';
import { searchMcpMarket } from '../../services/config';
import { useConfigStore } from '../../stores/configStore';
import { errText, toast } from '../../utils/feedback';
import { Empty } from '../common/Empty';

/** 安装时写入的服务器名：title 的 slug，兜底 name 最后一段；合法且不超长 */
function slugifyName(item: McpMarketItemDTO): string {
  const lastSeg = item.name.split('/').pop() ?? item.name;
  const base = item.title || lastSeg;
  const slug = base
    .toLowerCase()
    .replace(/[^a-z0-9_-]+/g, '-')
    .replace(/^-+|-+$/g, '');
  const fallback = lastSeg.replace(/[^a-zA-Z0-9_-]/g, '');
  return (slug || fallback || 'mcp-server').slice(0, 40);
}

/** MCP 市场视图：精选 + registry 搜索 + 一键安装（复用 save_mcp_server） */
export function McpMarketView() {
  const servers = useConfigStore((s) => s.mcpServers);
  const saveMcpServer = useConfigStore((s) => s.saveMcpServer);
  const loadMcpServers = useConfigStore((s) => s.loadMcpServers);

  const [keyword, setKeyword] = useState('');
  const [items, setItems] = useState<McpMarketItemDTO[]>([]);
  const [loading, setLoading] = useState(false);
  const [searched, setSearched] = useState(false);
  /** 安装中的条目名 */
  const [installing, setInstalling] = useState<string | null>(null);

  const search = async (kw: string) => {
    setLoading(true);
    try {
      setItems(await searchMcpMarket(kw));
      setSearched(true);
    } catch (e) {
      toast.error(`搜索 MCP 市场失败：${errText(e)}`);
    } finally {
      setLoading(false);
    }
  };

  // 首次进入自动用空 keyword 加载精选
  useEffect(() => {
    void search('');
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 已安装判定：与安装时写入的 slug name 比对
  const installedNames = useMemo(() => new Set(servers.map((s) => s.name)), [servers]);

  const onInstall = async (item: McpMarketItemDTO) => {
    if (!item.command) return;
    const name = slugifyName(item);
    setInstalling(item.name);
    // 安装复用 save_mcp_server（新增不带 id）；安装后是 disabled 待启用
    const ok = await saveMcpServer(undefined, name, item.command);
    setInstalling(null);
    if (ok) {
      toast.success(`已安装 ${item.title || name}，到「已安装」里启用`);
      void loadMcpServers();
    }
  };

  return (
    <div>
      <Input.Search
        placeholder="搜索 MCP 服务器…"
        allowClear
        value={keyword}
        loading={loading}
        onChange={(e) => setKeyword(e.target.value)}
        onSearch={(v) => void search(v.trim())}
        style={{ marginBottom: 12 }}
      />
      <Spin spinning={loading && !searched}>
        <div className="market-list">
          {items.length === 0 && !loading && searched ? (
            <Empty text="没有匹配的 MCP 服务器" />
          ) : (
            items.map((it) => {
              const installed = installedNames.has(slugifyName(it));
              const remote = it.command === null;
              return (
                <div className="market-card" key={`${it.source}:${it.name}`}>
                  <div className="mc-main">
                    <div className="mc-head">
                      <span className="mc-name">{it.title || it.name}</span>
                      <span className="mc-name-sub mono">{it.name}</span>
                      {it.version ? <span className="mc-version mono">v{it.version}</span> : null}
                      {it.source === 'featured' ? <Tag color="cyan">精选</Tag> : null}
                      {it.homepage ? (
                        <a
                          className="mc-home"
                          href={it.homepage}
                          target="_blank"
                          rel="noreferrer"
                          title={it.homepage}
                        >
                          <LinkOutlined />
                        </a>
                      ) : null}
                    </div>
                    <div className="mc-desc" title={it.description}>
                      {it.description}
                    </div>
                  </div>
                  {remote ? (
                    <Tooltip title="远程服务暂不支持一键安装">
                      <Button disabled icon={<DownloadOutlined />}>
                        安装
                      </Button>
                    </Tooltip>
                  ) : (
                    <Button
                      type={installed ? 'default' : 'primary'}
                      icon={<DownloadOutlined />}
                      loading={installing === it.name}
                      disabled={installed}
                      onClick={() => void onInstall(it)}
                    >
                      {installed ? '已安装' : '安装'}
                    </Button>
                  )}
                </div>
              );
            })
          )}
        </div>
      </Spin>
    </div>
  );
}
