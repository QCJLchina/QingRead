import Icon from "./icons";
import { THEME_CYCLE, THEME_LABELS } from "../store/settings";
import type { AppSettings, ReaderMode } from "../types";

interface ReadingSettingsPanelProps {
  settings: AppSettings;
  mode: ReaderMode;
  onClose: () => void;
  onChange: (partial: Partial<AppSettings>) => void;
  onModeChange: (mode: ReaderMode) => void;
}

const FONTS: { label: string; value: string }[] = [
  { label: "系统字体", value: "system-ui, -apple-system, sans-serif" },
  { label: "宋体 · 书页感", value: "Georgia, 'Songti SC', 'SimSun', serif" },
  { label: "黑体 · 屏幕感", value: "'Microsoft YaHei', 'PingFang SC', sans-serif" },
  { label: "等宽", value: "'Cascadia Mono', 'Consolas', monospace" },
];

export default function ReadingSettingsPanel(props: ReadingSettingsPanelProps) {
  const { settings, mode, onClose, onChange, onModeChange } = props;

  return (
    <div className="reader-panel" role="dialog" aria-label="阅读设置">
      <header className="panel-header">
        <strong>阅读设置</strong>
        <button className="icon-button" onClick={onClose} aria-label="关闭阅读设置">
          <Icon name="close" size={15} />
        </button>
      </header>

      <div className="panel-body">
        <div className="field">
          <label>阅读方式</label>
          <div className="segmented">
            <button
              className={mode === "paged" ? "is-active" : ""}
              aria-pressed={mode === "paged"}
              onClick={() => onModeChange("paged")}
            >
              分页
            </button>
            <button
              className={mode === "scroll" ? "is-active" : ""}
              aria-pressed={mode === "scroll"}
              onClick={() => onModeChange("scroll")}
            >
              滚动
            </button>
          </div>
        </div>

        <div className="field">
          <label>
            字号
            <output>{settings.font_size} px</output>
          </label>
          <input
            type="range"
            min={12}
            max={32}
            step={1}
            value={settings.font_size}
            aria-label="字号"
            onChange={(event) => onChange({ font_size: Number(event.target.value) })}
          />
        </div>

        <div className="field">
          <label>
            行距
            <output>{settings.line_height.toFixed(1)}</output>
          </label>
          <input
            type="range"
            min={1.2}
            max={3}
            step={0.1}
            value={settings.line_height}
            aria-label="行距"
            onChange={(event) => onChange({ line_height: Number(event.target.value) })}
          />
        </div>

        <div className="field">
          <label>字体</label>
          <select
            value={settings.font_family}
            aria-label="字体"
            onChange={(event) => onChange({ font_family: event.target.value })}
          >
            {FONTS.map((font) => (
              <option key={font.value} value={font.value}>
                {font.label}
              </option>
            ))}
          </select>
        </div>

        <div className="field">
          <label>
            正文栏宽
            <output>{settings.content_width ? settings.content_width + " px" : "跟随窗口"}</output>
          </label>
          <input
            type="range"
            min={0}
            max={900}
            step={20}
            value={settings.content_width ?? 0}
            aria-label="正文栏宽"
            onChange={(event) => {
              const value = Number(event.target.value);
              onChange({ content_width: value === 0 ? null : value });
            }}
          />
          <p className="field-hint">窗口拉宽时限制每行长度，避免一行读太久。</p>
        </div>

        <div className="field">
          <label>
            页边距
            <output>{settings.content_padding ?? (mode === "paged" ? 32 : 40)} px</output>
          </label>
          <input
            type="range"
            min={8}
            max={80}
            step={2}
            value={settings.content_padding ?? (mode === "paged" ? 32 : 40)}
            aria-label="页边距"
            onChange={(event) => onChange({ content_padding: Number(event.target.value) })}
          />
        </div>

        <div className="field">
          <label>配色</label>
          <div className="segmented theme-segmented">
            {THEME_CYCLE.map((theme) => (
              <button
                key={theme}
                className={settings.theme === theme ? "is-active" : ""}
                aria-pressed={settings.theme === theme}
                onClick={() => onChange({ theme })}
              >
                {THEME_LABELS[theme]}
              </button>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
