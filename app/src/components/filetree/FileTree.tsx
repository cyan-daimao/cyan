import { useCallback, useEffect, useState } from 'react';
import type { FileNodeDTO } from '../../types';
import { FileNode } from './FileNode';

interface FileTreeProps {
  nodes: FileNodeDTO[];
  onFile: (path: string) => void;
  /** 缓存键（通常为项目路径）：按项目持久化目录展开状态 */
  treeKey?: string;
}

const cacheKey = (treeKey: string) => `cyan.filetree.expanded.${treeKey}`;

/** 读取缓存的展开集合（无缓存 = 全部收起） */
function loadExpanded(treeKey: string | undefined): Set<string> {
  if (!treeKey) return new Set();
  try {
    const raw = localStorage.getItem(cacheKey(treeKey));
    const arr: unknown = raw ? JSON.parse(raw) : [];
    return new Set(Array.isArray(arr) ? (arr as string[]) : []);
  } catch {
    return new Set();
  }
}

/** 文件树：维护目录展开集合，按项目缓存（刷新/切项目后恢复上次状态；默认收起） */
export function FileTree({ nodes, onFile, treeKey }: FileTreeProps) {
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(() => loadExpanded(treeKey));

  // 换树（刷新/切项目）时恢复该项目上次缓存的展开状态
  useEffect(() => {
    setExpanded(loadExpanded(treeKey));
  }, [treeKey, nodes]);

  const toggleDir = useCallback(
    (path: string) => {
      setExpanded((prev) => {
        const next = new Set(prev);
        if (next.has(path)) next.delete(path);
        else next.add(path);
        if (treeKey) {
          try {
            localStorage.setItem(cacheKey(treeKey), JSON.stringify([...next]));
          } catch {
            // 存储满等异常忽略，展开态不持久化不影响使用
          }
        }
        return next;
      });
    },
    [treeKey],
  );

  if (nodes.length === 0) {
    return <div className="session-empty">目录为空</div>;
  }

  return (
    <>
      {nodes.map((n) => (
        <FileNode
          key={n.path}
          node={n}
          depth={0}
          expanded={expanded}
          onToggleDir={toggleDir}
          onFile={onFile}
        />
      ))}
    </>
  );
}
