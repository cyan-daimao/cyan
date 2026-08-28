import { Alert, Segmented } from 'antd';
import { CheckOutlined, MoonOutlined, SunOutlined } from '@ant-design/icons';
import { THEME_COLORS, useConfigStore } from '../../stores/configStore';
import type { BgMode, Lang } from '../../stores/configStore';
import { toast } from '../../utils/feedback';

/** 设置 - 主题：界面语言（antd 组件级）+ 主题色 + 背景主题 */
export function ThemeTab() {
  const lang = useConfigStore((s) => s.lang);
  const setLang = useConfigStore((s) => s.setLang);
  const themeColor = useConfigStore((s) => s.themeColor);
  const setThemeColor = useConfigStore((s) => s.setThemeColor);
  const bgMode = useConfigStore((s) => s.bgMode);
  const setBgMode = useConfigStore((s) => s.setBgMode);

  const onLang = (v: string | number) => {
    const next = v as Lang;
    setLang(next);
    toast.info(next === 'zh' ? '已切换到中文' : 'Switched to English');
  };

  return (
    <div>
      <Alert
        type="info"
        showIcon
        style={{ marginBottom: 16 }}
        message="语言作用于界面组件（表格分页、弹窗按钮等）；应用自身的功能文案暂以中文为准。主题色即时生效并本地持久化。"
      />
      <div className="theme-block">
        <div className="theme-label">语言 / Language</div>
        <Segmented
          value={lang}
          onChange={onLang}
          options={[
            { label: '中文', value: 'zh' },
            { label: 'English', value: 'en' },
          ]}
        />
      </div>
      <div className="theme-block">
        <div className="theme-label">背景主题</div>
        <Segmented
          value={bgMode}
          onChange={(v) => setBgMode(v as BgMode)}
          options={[
            { label: '浅色', value: 'light', icon: <SunOutlined /> },
            { label: '深色', value: 'dark', icon: <MoonOutlined /> },
          ]}
        />
      </div>
      <div className="theme-block">
        <div className="theme-label">主题色</div>
        <div className="theme-swatches">
          {THEME_COLORS.map((c) => (
            <button
              key={c.value}
              className={`swatch${themeColor === c.value ? ' active' : ''}`}
              style={{ background: c.value }}
              title={c.name}
              onClick={() => setThemeColor(c.value)}
            >
              {themeColor === c.value ? <CheckOutlined /> : null}
            </button>
          ))}
        </div>
        <div className="theme-hint">
          当前：{THEME_COLORS.find((c) => c.value === themeColor)?.name ?? themeColor}
        </div>
      </div>
    </div>
  );
}
