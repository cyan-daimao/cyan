import type { FileNodeDTO } from '../../types';

/** 按扩展名分配文件图标 */
export function fileIcon(name: string): string {
  if (name.endsWith('.ts') || name.endsWith('.tsx')) return '🔷';
  if (name.endsWith('.js') || name.endsWith('.jsx')) return '🟨';
  if (name.endsWith('.json')) return '🧾';
  if (name.endsWith('.md')) return '📝';
  if (name.endsWith('.rs')) return '🦀';
  if (name.endsWith('.toml') || name.endsWith('.yaml') || name.endsWith('.yml')) return '⚙️';
  if (name.endsWith('.css') || name.endsWith('.html')) return '🎨';
  return '📄';
}

interface FileNodeProps {
  node: FileNodeDTO;
  depth: number;
  expanded: ReadonlySet<string>;
  onToggleDir: (path: string) => void;
  onFile: (path: string) => void;
}

/** 文件树节点（递归）：目录展开/收起，文件点击预览；路径取自后端 node.path */
export function FileNode({ node, depth, expanded, onToggleDir, onFile }: FileNodeProps) {
  const pad = { paddingLeft: 8 + depth * 16 };
  if (node.isDir) {
    const open = expanded.has(node.path);
    return (
      <div>
        <div
          className={`ft-row ft-dir${open ? ' open' : ''}`}
          style={pad}
          onClick={() => onToggleDir(node.path)}
        >
          <span className="ft-caret">▶</span>
          <span className="ft-icon">{open ? '📂' : '📁'}</span>
          <span className="ft-name" title={node.name}>
            {node.name}
          </span>
        </div>
        {open
          ? node.children.map((c) => (
              <FileNode
                key={c.path}
                node={c}
                depth={depth + 1}
                expanded={expanded}
                onToggleDir={onToggleDir}
                onFile={onFile}
              />
            ))
          : null}
      </div>
    );
  }
  return (
    <div className="ft-row ft-file" style={pad} onClick={() => onFile(node.path)}>
      <span className="ft-caret" />
      <span className="ft-icon">{fileIcon(node.name)}</span>
      <span className="ft-name" title={node.name}>
        {node.name}
      </span>
    </div>
  );
}
