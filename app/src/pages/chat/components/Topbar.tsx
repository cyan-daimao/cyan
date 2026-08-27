interface TopbarProps {
  /** <1100px 时隐藏文件目录开关 */
  showFiles: boolean;
  filesActive: boolean;
  onToggleFiles: () => void;
  onOpenDrawer: () => void;
}

/** 极简顶栏：文件目录开关 🗂 + 任务与变更开关 📋 */
export function Topbar({ showFiles, filesActive, onToggleFiles, onOpenDrawer }: TopbarProps) {
  return (
    <header className="topbar">
      <div className="spacer" />
      {showFiles ? (
        <button
          className={`icon-btn${filesActive ? ' active' : ''}`}
          title="文件目录"
          onClick={onToggleFiles}
        >
          🗂
        </button>
      ) : null}
      <button className="icon-btn" title="任务与变更" onClick={onOpenDrawer}>
        📋
      </button>
    </header>
  );
}
