import { useEffect, useMemo, useState } from 'react';
import { useNavigate, useParams } from 'react-router-dom';
import { Button, Spin, Table } from 'antd';
import { ArrowLeftOutlined } from '@ant-design/icons';
import { listSessions, projectTokenUsage } from '../../services/session';
import { useProjectStore } from '../../stores/projectStore';
import { errText, toast } from '../../utils/feedback';
import { fmtTokens } from '../../utils/format';
import type { ProjectTokenUsageDTO, SessionSummaryDTO } from '../../types';

/** 会话用量行（表格数据源） */
interface UsageRow {
  id: number;
  title: string;
  input: number;
  output: number;
  total: number;
  ctx: number;
  updatedAt: string;
}

/** Token 用量报表页：汇总卡片 + 按会话用量表格（占比条形） */
export default function UsagePage() {
  const { projectPath = '' } = useParams();
  const path = decodeURIComponent(projectPath);
  const navigate = useNavigate();
  const recents = useProjectStore((s) => s.recents);

  const [usage, setUsage] = useState<ProjectTokenUsageDTO | null>(null);
  const [rows, setRows] = useState<UsageRow[]>([]);
  const [loading, setLoading] = useState(true);

  const projectName = useMemo(() => {
    const hit = recents.find((p) => p.path === path);
    return hit?.name ?? path.split('/').filter(Boolean).pop() ?? path;
  }, [recents, path]);

  useEffect(() => {
    if (!path) return;
    let cancelled = false;
    setLoading(true);
    void (async () => {
      try {
        const [u, sessions] = await Promise.all([projectTokenUsage(path), listSessions(path)]);
        if (cancelled) return;
        setUsage(u);
        setRows(
          sessions
            .map((s: SessionSummaryDTO) => ({
              id: s.id,
              title: s.title,
              input: s.tokens.input,
              output: s.tokens.output,
              total: s.tokens.input + s.tokens.output,
              ctx: s.ctx,
              updatedAt: s.updatedAt,
            }))
            .sort((a, b) => b.total - a.total),
        );
      } catch (e) {
        toast.error(`加载用量报表失败：${errText(e)}`);
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [path]);

  const grandTotal = (usage?.inputTokens ?? 0) + (usage?.outputTokens ?? 0);

  return (
    <div className="usage-page">
      <div className="usage-head">
        <Button icon={<ArrowLeftOutlined />} onClick={() => navigate('/chat')}>
          返回
        </Button>
        <div className="usage-title">
          Token 用量报表 <span className="usage-sub mono">{projectName}</span>
        </div>
      </div>
      <Spin spinning={loading}>
        <div className="usage-cards">
          <div className="usage-card">
            <div className="uc-num">{usage?.sessionCount ?? 0}</div>
            <div className="uc-label">会话数</div>
          </div>
          <div className="usage-card">
            <div className="uc-num">↑ {fmtTokens(usage?.inputTokens ?? 0)}</div>
            <div className="uc-label">累计输入</div>
          </div>
          <div className="usage-card">
            <div className="uc-num">↓ {fmtTokens(usage?.outputTokens ?? 0)}</div>
            <div className="uc-label">累计输出</div>
          </div>
          <div className="usage-card">
            <div className="uc-num">{fmtTokens(grandTotal)}</div>
            <div className="uc-label">总计 tokens</div>
          </div>
        </div>
        <Table<UsageRow>
          rowKey="id"
          dataSource={rows}
          pagination={{ pageSize: 10, hideOnSinglePage: true }}
          locale={{ emptyText: '暂无会话用量数据' }}
          columns={[
            {
              title: '会话',
              dataIndex: 'title',
              render: (title: string) => <span>{title}</span>,
            },
            {
              title: '输入 ↑',
              dataIndex: 'input',
              width: 110,
              align: 'right',
              sorter: (a, b) => a.input - b.input,
              render: (v: number) => fmtTokens(v),
            },
            {
              title: '输出 ↓',
              dataIndex: 'output',
              width: 110,
              align: 'right',
              sorter: (a, b) => a.output - b.output,
              render: (v: number) => fmtTokens(v),
            },
            {
              title: '占比',
              dataIndex: 'total',
              render: (v: number) => {
                const pct = grandTotal > 0 ? Math.max(2, Math.round((v / grandTotal) * 100)) : 0;
                return (
                  <div className="usage-bar-cell">
                    <div className="usage-bar">
                      <i style={{ width: `${pct}%` }} />
                    </div>
                    <span>{grandTotal > 0 ? `${Math.round((v / grandTotal) * 100)}%` : '—'}</span>
                  </div>
                );
              },
            },
            { title: '更新时间', dataIndex: 'updatedAt', width: 170 },
          ]}
        />
      </Spin>
    </div>
  );
}
