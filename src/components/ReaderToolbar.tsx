import Icon from "./icons";
import type { ReaderPanel } from "../store/window";
import type { ReaderMode } from "../types";

interface ReaderToolbarProps {
  bookTitle: string;
  chapterTitle: string;
  chapterIndex: number;
  chapterCount: number;
  mode: ReaderMode;
  narrow: boolean;
  lowDistraction: boolean;
  sidebarVisible: boolean;
  panel: ReaderPanel;
  hidden: boolean;
  onBack: () => void;
  onToggleSidebar: () => void;
  onOpenPanel: (panel: ReaderPanel) => void;
  onToggleMode: () => void;
  onToggleLowDistraction: () => void;
  alwaysOnTop: boolean;
  onToggleAlwaysOnTop: () => void;
  onCurtain: () => void;
  onHideWindow: () => void;
  onRestoreToolbar: () => void;
}

export default function ReaderToolbar(props: ReaderToolbarProps) {
  const {
    bookTitle,
    chapterTitle,
    chapterIndex,
    chapterCount,
    mode,
    narrow,
    lowDistraction,
    sidebarVisible,
    panel,
    hidden,
    onBack,
    onToggleSidebar,
    onOpenPanel,
    onToggleMode,
    onToggleLowDistraction,
    alwaysOnTop,
    onToggleAlwaysOnTop,
    onCurtain,
    onHideWindow,
    onRestoreToolbar,
  } = props;

  if (hidden) {
    return (
      <button className="reader-toolbar-handle" onClick={onRestoreToolbar} aria-label="显示工具栏">
        <Icon name="chevron-down" size={14} />
      </button>
    );
  }

  const longTitle = lowDistraction ? "" : bookTitle + " · " + chapterTitle;

  return (
    <div className={"reader-toolbar" + (narrow ? " is-narrow" : "")}>
      <div className="toolbar-group">
        <button className="icon-button with-label" onClick={onBack} title="返回书架（不丢失当前进度）">
          <Icon name="arrow-left" />
          {!narrow && <span>书架</span>}
        </button>
        <button
          className={"icon-button" + (sidebarVisible ? " is-active" : "")}
          onClick={onToggleSidebar}
          aria-pressed={sidebarVisible}
          title="目录"
        >
          <Icon name="panel-left" />
        </button>
      </div>

      <div className="toolbar-title" title={longTitle}>
        {longTitle && <span className="toolbar-book">{longTitle}</span>}
        {!narrow && chapterCount > 0 && (
          <span className="toolbar-chapter">第 {chapterIndex + 1} / {chapterCount} 章</span>
        )}
      </div>

      <div className="toolbar-group">
        <button
          className="icon-button"
          onClick={onToggleMode}
          title={mode === "paged" ? "切换到滚动阅读" : "切换到分页阅读"}
        >
          <Icon name={mode === "paged" ? "list" : "column"} />
        </button>
        <button
          className={"icon-button" + (panel === "reading" ? " is-active" : "")}
          onClick={() => onOpenPanel("reading")}
          title="阅读设置"
        >
          <Icon name="type" />
        </button>
        <button
          className={"icon-button" + (panel === "window" ? " is-active" : "")}
          onClick={() => onOpenPanel("window")}
          title="窗口尺寸"
        >
          <Icon name="scan" />
        </button>
        <button
          className={"icon-button" + (panel === "search" ? " is-active" : "")}
          onClick={() => onOpenPanel("search")}
          title="搜索"
        >
          <Icon name="search" />
        </button>
        <button
          className={"icon-button" + (alwaysOnTop ? " is-active" : "")}
          onClick={onToggleAlwaysOnTop}
          aria-pressed={alwaysOnTop}
          title="窗口置顶"
        >
          <Icon name="pin" />
        </button>
        <button
          className={"icon-button" + (lowDistraction ? " is-active" : "")}
          onClick={onToggleLowDistraction}
          aria-pressed={lowDistraction}
          title="低干扰模式"
        >
          <Icon name="leaf" />
        </button>
        <button className="icon-button" onClick={onCurtain} title="遮挡阅读内容">
          <Icon name="eye-off" />
        </button>
        {!narrow && (
          <button className="icon-button" onClick={onHideWindow} title="隐藏窗口">
            <Icon name="minus" />
          </button>
        )}
      </div>
    </div>
  );
}
