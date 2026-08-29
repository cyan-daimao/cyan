import { useCallback, useEffect, useState } from 'react';
import type { FileNodeDTO } from '../../types';
import { FileNode } from './FileNode';

interface FileTreeProps {
  nodes: FileNodeDTO[];
  onFile: (path: string) => void;
}

/** 文件树：维护目录展开集合，默认全部收起（点击目录名展开） */
export function FileTree({ nodes, onFile }: FileTreeProps) {
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(new Set());

  // 换树（刷新/切项目）时重置为全部收起
  useEffect(() => {
    setExpanded(new Set());
  }, [nodes]);

  const toggleDir = useCallback((path: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  }, []);

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
