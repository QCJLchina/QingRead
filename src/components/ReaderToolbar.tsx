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
  onAddBookmark: () => void;
  onOpenBookmarks: () => void;
  onOpenHistory: () => void;
  hasBookmarkHere: boolean;
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
    onAddBookmark,
    onOpenBookmarks,
    onOpenHistory,
    hasBookmarkHere,
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
        {narrow && (
          <details className="toolbar-menu">
            <summary className="icon-button" aria-label="更多阅读操作">⋯</summary>
            <div className="toolbar-menu-items" onClick={(event) => {
              const details = event.currentTarget.closest("details");
              if (details) details.open = false;
            }}>
              <button onClick={onAddBookmark}>添加书签</button>
              <button onClick={onOpenBookmarks}>书签列表</button>
              <button onClick={onOpenHistory}>阅读历史</button>
              <button onClick={() => onOpenPanel("search")}>搜索</button>
              <button onClick={onToggleMode}>切换阅读方式</button>
              <button onClick={() => onOpenPanel("reading")}>阅读设置</button>
              <button onClick={() => onOpenPanel("window")}>窗口尺寸</button>
              <button onClick={onToggleAlwaysOnTop}>切换置顶</button>
              <button onClick={onToggleLowDistraction}>低干扰模式</button>
              <button onClick={onCurtain}>遮挡正文</button>
              <button onClick={onHideWindow}>隐藏窗口</button>
            </div>
          </details>
        )}
        {!narrow && <>
        <button
          className="icon-button"
          onClick={onAddBookmark}
          title={hasBookmarkHere ? "此处已有书签" : "添加书签"}
        >
          <Icon name="bookmark" />
        </button>
        <button
          className={"icon-button" + (panel === "bookmarks" ? " is-active" : "")}
          onClick={() => onOpenPanel("bookmarks")}
          title="书签列表"
        >
          <Icon name="list" />
        </button>
        <button
          className={"icon-button" + (panel === "history" ? " is-active" : "")}
          onClick={() => onOpenPanel("history")}
          title="阅读历史"
        >
          <Icon name="clock" />
        </button>
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
        </>}
      </div>
    </div>
  );
}
