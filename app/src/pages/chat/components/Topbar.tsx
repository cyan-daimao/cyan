import { FolderOpenOutlined, GlobalOutlined, ProfileOutlined } from '@ant-design/icons';

interface TopbarProps {
  /** <1100px 时隐藏文件目录开关 */
  showFiles: boolean;
  filesActive: boolean;
  onToggleFiles: () => void;
  onOpenDrawer: () => void;
  /** 浏览器面板开关（右侧 panel） */
  browserActive: boolean;
  onToggleBrowser: () => void;
}

/** 极简顶栏：浏览器面板 / 文件目录 / 任务与变更 三个开关 */
export function Topbar({
  showFiles,
  filesActive,
  onToggleFiles,
  onOpenDrawer,
  browserActive,
  onToggleBrowser,
}: TopbarProps) {
  return (
    <header className="topbar">
      <div className="spacer" />
      <button
        className={`icon-btn${browserActive ? ' active' : ''}`}
        title="浏览器面板（与 agent 共享同一受控浏览器）"
        onClick={onToggleBrowser}
      >
        <GlobalOutlined />
      </button>
      {showFiles ? (
        <button
          className={`icon-btn${filesActive ? ' active' : ''}`}
          title="文件目录"
          onClick={onToggleFiles}
        >
          <FolderOpenOutlined />
        </button>
      ) : null}
      <button className="icon-btn" title="任务与变更" onClick={onOpenDrawer}>
        <ProfileOutlined />
      </button>
    </header>
  );
}
