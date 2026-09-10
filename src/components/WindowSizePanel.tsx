import Icon from "./icons";
import { MIN_WINDOW_HEIGHT, MIN_WINDOW_WIDTH, WINDOW_PRESETS, applyLockedRatio } from "../lib/window-presets";
import type { WindowSettings } from "../types";

interface WindowSizePanelProps {
  settings: WindowSettings;
  message?: string | null;
  onClose: () => void;
  onApplyPreset: (id: WindowSettings["layout"]) => void;
  onApplySize: (width: number, height: number) => void;
  onChange: (partial: Partial<WindowSettings>) => void;
}

export default function WindowSizePanel(props: WindowSizePanelProps) {
  const { settings, message, onClose, onApplyPreset, onApplySize, onChange } = props;
  // 锁定比例时，以当前窗口形状为准，只按用户这一侧改动重新算另一侧
  const ratio = settings.height > 0 ? settings.width / settings.height : 0;

  return (
    <div className="reader-panel" role="dialog" aria-label="窗口尺寸">
      <header className="panel-header">
        <strong>窗口尺寸</strong>
        <button className="icon-button" onClick={onClose} aria-label="关闭窗口尺寸">
          <Icon name="close" size={15} />
        </button>
      </header>

      <div className="panel-body">
        <div className="field">
          <label>常用形态</label>
          <div className="preset-list">
            {WINDOW_PRESETS.map((preset) => (
              <button
                key={preset.id}
                className={"preset-card" + (settings.layout === preset.id ? " is-active" : "")}
                aria-pressed={settings.layout === preset.id}
                onClick={() => onApplyPreset(preset.id)}
              >
                <span className="preset-label">{preset.label}</span>
                <span className="preset-size">
                  {preset.width} × {preset.height}
                </span>
                <span className="preset-desc">{preset.description}</span>
              </button>
            ))}
          </div>
        </div>

        <div className="field">
          <label>自定义宽高 · 逻辑像素</label>
          <div className="size-inputs">
            <input
              type="number"
              aria-label="窗口宽度"
              min={MIN_WINDOW_WIDTH}
              step={10}
              value={Math.round(settings.width)}
              onChange={(event) => {
                const width = Number(event.target.value);
                if (!Number.isFinite(width)) return;
                const next = settings.lock_ratio
                  ? applyLockedRatio(width, settings.height, ratio, "width")
                  : { width, height: settings.height };
                onApplySize(next.width, next.height);
              }}
            />
            <span>×</span>
            <input
              type="number"
              aria-label="窗口高度"
              min={MIN_WINDOW_HEIGHT}
              step={10}
              value={Math.round(settings.height)}
              onChange={(event) => {
                const height = Number(event.target.value);
                if (!Number.isFinite(height)) return;
                const next = settings.lock_ratio
                  ? applyLockedRatio(settings.width, height, ratio, "height")
                  : { width: settings.width, height };
                onApplySize(next.width, next.height);
              }}
            />
          </div>
          <label className="switch-row">
            <input
              type="checkbox"
              checked={settings.lock_ratio}
              onChange={(event) => onChange({ lock_ratio: event.target.checked })}
            />
            锁定当前长宽比例
          </label>
          <p className="field-hint">
            最小 {MIN_WINDOW_WIDTH} × {MIN_WINDOW_HEIGHT}，上限不超过当前显示器工作区。
          </p>
        </div>

        <div className="field">
          <label className="switch-row">
            <input
              type="checkbox"
              checked={settings.always_on_top}
              onChange={(event) => onChange({ always_on_top: event.target.checked })}
            />
            窗口置顶
          </label>
          <label className="switch-row">
            <input
              type="checkbox"
              checked={settings.low_distraction}
              onChange={(event) => onChange({ low_distraction: event.target.checked })}
            />
            低干扰模式（中性标题、隐藏装饰背景）
          </label>
          <label className="switch-row">
            <input
              type="checkbox"
              checked={settings.toolbar_auto_hide}
              onChange={(event) => onChange({ toolbar_auto_hide: event.target.checked })}
            />
            鼠标移开后自动收起工具栏
          </label>
          <label className="switch-row">
            <input
              type="checkbox"
              checked={settings.blur_curtain}
              onChange={(event) => onChange({ blur_curtain: event.target.checked })}
            />
            切到别的程序时遮挡正文
          </label>
        </div>

        <div className="field">
          <label>隐藏 / 恢复快捷键</label>
          <input
            type="text"
            value={settings.hide_hotkey}
            aria-label="隐藏恢复快捷键"
            placeholder="Ctrl+Alt+H"
            onChange={(event) => onChange({ hide_hotkey: event.target.value })}
          />
          <p className="field-hint">形如 Ctrl+Alt+H、Ctrl+Shift+F2。被系统或其他程序占用时会给出提示。</p>
        </div>

        {message && <p className="panel-message" role="status" aria-live="polite">{message}</p>}
      </div>
    </div>
  );
}
