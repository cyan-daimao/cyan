import { Alert, Segmented } from 'antd';
import { BROWSER_HOMES, useConfigStore } from '../../stores/configStore';
import type { BrowserHomeKey } from '../../stores/configStore';
import { browserHomeUrl } from '../../stores/configStore';

/** 设置 - 浏览器：内置浏览器默认主页（关闭重开面板后生效） */
export function BrowserTab() {
  const browserHome = useConfigStore((s) => s.browserHome);
  const setBrowserHome = useConfigStore((s) => s.setBrowserHome);

  return (
    <div>
      <Alert
        type="info"
        showIcon
        style={{ marginBottom: 16 }}
        message="内置浏览器打开时默认加载的页面；已打开的浏览器面板不会受影响，关闭后重新打开生效。"
      />
      <div className="theme-block">
        <div className="theme-label">默认主页</div>
        <Segmented
          value={browserHome}
          onChange={(v) => setBrowserHome(v as BrowserHomeKey)}
          options={BROWSER_HOMES.map((h) => ({ label: h.name, value: h.key }))}
        />
        <div className="theme-hint">当前：{browserHomeUrl(browserHome)}</div>
      </div>
    </div>
  );
}
