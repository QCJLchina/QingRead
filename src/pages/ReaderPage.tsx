import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { listen } from "@tauri-apps/api/event";
import CurtainOverlay from "../components/CurtainOverlay";
import Icon from "../components/icons";
import ReaderSidebar from "../components/ReaderSidebar";
import ReaderToolbar from "../components/ReaderToolbar";
import ReadingSettingsPanel from "../components/ReadingSettingsPanel";
import SearchPanel from "../components/SearchPanel";
import BookmarksPanel from "../components/BookmarksPanel";
import HistoryPanel from "../components/HistoryPanel";
import WindowSizePanel from "../components/WindowSizePanel";
import { useReaderLayout } from "../hooks/useReaderLayout";
import type { ReaderLayoutInfo } from "../hooks/useReaderLayout";
import { useReaderNavigation } from "../hooks/useReaderNavigation";
import { clearIndex } from "../lib/search";
import {
  anchorAtPlainText,
  applyHighlightAtPosition,
  applyMatchHighlight,
  clearMatchHighlight,
} from "../lib/highlight";
import { estimateBookProgress } from "../lib/pagination";
import { useLibraryStore } from "../store/library";
import { useReaderStore, flushProgressWrites } from "../store/reader";
import { useReadingStore, flushReadingWrites } from "../store/reading";
import { registerClosePreparation } from "../lib/close-preparation";
import {
  backgroundImageCss,
  resolveContentPadding,
  resolveReadingMode,
  useSettingsStore,
} from "../store/settings";
import { useWindowStore } from "../store/window";
import { windowApi } from "../api";
import type { Bookmark, HistoryEntry, ReaderMode, TextAnchor } from "../types";

/** 正文可用宽度低于这个值时，目录和工具栏改用浮层 / 精简形态 */
const NARROW_WIDTH = 600;
const SAVE_DEBOUNCE_MS = 500;

export default function ReaderPage() {
  const { bookId } = useParams<{ bookId: string }>();
  const navigate = useNavigate();

  const {
    chapterCount,
    currentChapter,
    toc,
    loading,
    error,
    pendingPosition,
    openBook,
    loadChapter,
    preloadChapter,
    loadingChapter,
    saveProgress,
    clearReader,
  } = useReaderStore();

  const { settings, updateSettings, setReadingMode } = useSettingsStore();
  const {
    settings: windowSettings,
    narrow,
    sidebarOpen,
    panel,
    curtain,
    hotkeyNotice,
    toolbarHidden,
    load: loadWindowSettings,
    update: updateWindowSettings,
    applyPreset,
    applySize,
    setNarrow,
    toggleSidebar,
    closeSidebar,
    openPanel,
    closePanel,
    setCurtain,
    setHotkeyNotice,
    setToolbarHidden,
  } = useWindowStore();

  const books = useLibraryStore((state) => state.books);
  const loadBooks = useLibraryStore((state) => state.loadBooks);
  const readingData = useReadingStore((state) => (bookId ? state.data[bookId] : undefined));
  const loadReadingBook = useReadingStore((state) => state.loadBook);
  const ensureStatus = useReadingStore((state) => state.ensureStatus);
  const addBookmarkAction = useReadingStore((state) => state.addBookmark);
  const appendHistory = useReadingStore((state) => state.appendHistory);

  const rootRef = useRef<HTMLDivElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const trackRef = useRef<HTMLDivElement>(null);
  const saveTimerRef = useRef<number | null>(null);
  const [returnAnchor, setReturnAnchor] = useState<{ chapterIndex: number; anchor: TextAnchor } | null>(null);
  const [layoutInfo, setLayoutInfo] = useState<ReaderLayoutInfo>({
    page: 0,
    pageCount: 1,
    ratio: 0,
    anchor: null,
  });

  const mode: ReaderMode = resolveReadingMode(settings.reading_mode);
  const contentPadding = resolveContentPadding(settings.content_padding, mode);
  const lowDistraction = windowSettings.low_distraction;
  const bookTitle = useMemo(
    () => books.find((book) => book.id === bookId)?.title ?? "",
    [books, bookId],
  );

  const contentKey = bookId + "#" + (currentChapter?.index ?? -1);

  useEffect(() => {
    if (bookId) void openBook(bookId);
    return () => {
      openRecordedRef.current = null;
      clearReader();
      // 返回书架时释放索引：缓存只限当前书、当前应用会话
      clearIndex();
    };
  }, [bookId, openBook, clearReader]);

  useEffect(() => {
    if (books.length === 0) void loadBooks();
  }, [books.length, loadBooks]);

  useEffect(() => {
    if (bookId) void loadReadingBook(bookId).catch((error) => setHotkeyNotice(String(error)));
  }, [bookId, loadReadingBook]);

  useEffect(() => {
    void loadWindowSettings();
  }, [loadWindowSettings]);

  useEffect(() => {
    clearMatchHighlight();


  }, [contentKey]);




  useEffect(() => {
    if (!currentChapter) return;
    preloadChapter(currentChapter.index + 1);
    preloadChapter(currentChapter.index - 1);
  }, [currentChapter, preloadChapter]);

  // 低干扰模式使用中性窗口标题，避免任务栏上直接写出书名
  useEffect(() => {
    if (lowDistraction) {
      document.title = "随手记";
    } else {
      document.title = bookTitle ? "轻阅 · " + bookTitle : "轻阅";
    }
  }, [bookTitle, lowDistraction]);

  useEffect(() => {
    const element = rootRef.current;
    if (!element) return;
    const observer = new ResizeObserver((entries) => {
      const width = entries[0]?.contentRect.width ?? element.clientWidth;
      setNarrow(width < NARROW_WIDTH);
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, [setNarrow]);

  useEffect(() => {
    const ready = listen<string>("hotkey-ready", (event) => {
      setHotkeyNotice("隐藏 / 恢复快捷键已就绪：" + event.payload);
    });
    const unavailable = listen<string>("hotkey-unavailable", (event) => {
      setHotkeyNotice(
        "快捷键 " + event.payload + " 无法注册（可能被其他程序占用）。托盘图标仍可恢复窗口。",
      );
    });
    return () => {
      ready.then((unlisten) => unlisten());
      unavailable.then((unlisten) => unlisten());
    };
  }, [setHotkeyNotice]);

  useEffect(() => {
    if (!hotkeyNotice) return;
    const timer = window.setTimeout(() => setHotkeyNotice(null), 6000);
    return () => window.clearTimeout(timer);
  }, [hotkeyNotice, setHotkeyNotice]);

  const commitProgress = useCallback(
    (info: ReaderLayoutInfo) => {
      const chapter = useReaderStore.getState().currentChapter;
      if (!chapter) return;
      saveProgress({
        chapterIndex: chapter.index,
        pageInChapter: info.page,
        anchor: info.anchor,
        mode,
      });
    },
    [mode, saveProgress],
  );

  // 打开书籍后正文排版完成时，记录一次实际可见位置。
  const openRecordedRef = useRef<string | null>(null);

  const handleSettled = useCallback(
    (info: ReaderLayoutInfo) => {
      setLayoutInfo(info);
      const track = trackRef.current;
      if (saveTimerRef.current !== null) window.clearTimeout(saveTimerRef.current);
      saveTimerRef.current = window.setTimeout(() => {
        saveTimerRef.current = null;
        commitProgress(info);
      }, SAVE_DEBOUNCE_MS);

      if (bookId && currentChapter && info.anchor && openRecordedRef.current !== bookId) {
        openRecordedRef.current = bookId;
        void ensureStatus(bookId, "reading").catch((error) => setHotkeyNotice(String(error)));
        void appendHistory({
          bookId,
          kind: "open",
          chapterIndex: currentChapter.index,
          anchor: info.anchor,
          snippet: info.anchor.snippet,
        }).catch((error) => setHotkeyNotice(String(error)));
      }

      const jump = jumpRef.current;
      if (jump && jump.id === info.requestId && currentChapter?.index === jump.chapterIndex && bookId) {
        jumpRef.current = null;
        if (jump.search && track) applyHighlightAtPosition(track, jump.search.position, jump.search.length);
        setReturnAnchor(jump.origin);
        void (async () => {
          if (jump.origin) await appendHistory({ bookId, kind: "jump", chapterIndex: jump.origin.chapterIndex, anchor: jump.origin.anchor, snippet: jump.origin.anchor.snippet });
          if (info.anchor) await appendHistory({ bookId, kind: "jump", chapterIndex: jump.chapterIndex, anchor: info.anchor, snippet: info.anchor.snippet });
        })().catch((error) => setHotkeyNotice(String(error)));
        if (info.approximate) setHotkeyNotice("无法精确定位，已跳至附近位置");
      }
    },
    [appendHistory, bookId, commitProgress, currentChapter, ensureStatus, setHotkeyNotice],
  );

  const layout = useReaderLayout({
    containerRef,
    trackRef,
    mode,
    contentKey,
    ready: Boolean(currentChapter) && !loading,
    fontSize: settings.font_size,
    lineHeight: settings.line_height,
    fontFamily: settings.font_family,
    contentWidth: settings.content_width,
    contentPadding,
    pending: pendingPosition,
    onSettled: handleSettled,
  });

  const goChapter = useCallback(
    (delta: -1 | 1) => {
      if (!currentChapter) return;
      const next = currentChapter.index + delta;
      if (next < 0 || next >= chapterCount) return;
      void loadChapter(next, delta > 0 ? { page: 0, ratio: 0 } : { atEnd: true });
    },
    [chapterCount, currentChapter, loadChapter],
  );

  const movePage = useCallback(
    (direction: -1 | 1) => {
      const result = layout.goPage(direction);
      if (result === "next-chapter") goChapter(1);
      else if (result === "prev-chapter") goChapter(-1);
    },
    [goChapter, layout],
  );

  const panelOpen = panel !== "none";
  const blocked = useCallback(() => panelOpen || curtain, [curtain, panelOpen]);

  useReaderNavigation({
    mode,
    enabled: Boolean(currentChapter) && layout.settled,
    blocked,
    containerRef,
    move: movePage,
    onEscape: () => {
      if (panel !== "none") {
        closePanel();
        return;
      }
      if (sidebarOpen) closeSidebar();
    },
  });

  const persistBeforeLeave = useCallback(async () => {
    if (saveTimerRef.current !== null) {
      window.clearTimeout(saveTimerRef.current);
      saveTimerRef.current = null;
    }
    const snapshot = layout.snapshot();
    if (currentChapter && bookId) {
      commitProgress(snapshot);
      if (snapshot.anchor) await appendHistory({ bookId, kind: "leave", chapterIndex: currentChapter.index, anchor: snapshot.anchor, snippet: snapshot.anchor.snippet });
    }
    await Promise.all([flushProgressWrites(), flushReadingWrites()]);
  }, [appendHistory, bookId, commitProgress, currentChapter, layout]);

  useEffect(() => registerClosePreparation(persistBeforeLeave), [persistBeforeLeave]);

  const handleBackToShelf = () => {
    void persistBeforeLeave().then(() => navigate("/")).catch((error) => setHotkeyNotice("保存失败，未离开阅读：" + String(error)));
  };
  const jumpSequenceRef = useRef(0);
  const jumpRef = useRef<{ id: number; chapterIndex: number; origin: { chapterIndex: number; anchor: TextAnchor } | null; search?: { position: number; length: number } } | null>(null);
  const jumpTo = useCallback(async (chapterIndex: number, anchor?: TextAnchor, search?: { position: number; length: number }) => {
    if (!bookId || loading) return;
    const snapshot = layout.snapshot();
    const origin = currentChapter && snapshot.anchor ? { chapterIndex: currentChapter.index, anchor: snapshot.anchor } : null;
    const id = ++jumpSequenceRef.current;
    jumpRef.current = { id, chapterIndex, origin, search };
    if (!search) closePanel();
    const ok = await loadChapter(chapterIndex, { anchor, textPosition: search?.position, ratio: anchor?.ratio ?? 0, requestId: id });
    if (!ok && jumpRef.current?.id === id) jumpRef.current = null;
  }, [bookId, loading, currentChapter, layout, loadChapter, closePanel]);
  const handleToggleMode = useCallback(() => {
    void setReadingMode(mode === "paged" ? "scroll" : "paged");
  }, [mode, setReadingMode]);

  const hasBookmarkHere = useMemo(() => {
    if (!readingData || !currentChapter || !layoutInfo.anchor) return false;
    return readingData.bookmarks.some((item) =>
      item.chapter_index === currentChapter.index &&
      item.anchor.offset === layoutInfo.anchor?.offset &&
      JSON.stringify(item.anchor.path) === JSON.stringify(layoutInfo.anchor?.path));
  }, [currentChapter, readingData, layoutInfo.anchor]);

  const handleAddBookmark = useCallback(async () => {
    if (!bookId || !currentChapter) return;
    const snapshot = layout.snapshot();
    if (!snapshot.anchor) return;
    try {
      const result = await addBookmarkAction({
        bookId,
        chapterIndex: currentChapter.index,
        anchor: snapshot.anchor,
        snippet: snapshot.anchor.snippet,
      });
      setHotkeyNotice(result.created ? "已添加书签" : "此处已有书签");
      if (!result.created) openPanel("bookmarks");
    } catch {
      setHotkeyNotice("书签保存失败，请重试");
    }
  }, [addBookmarkAction, bookId, currentChapter, layout, setHotkeyNotice, openPanel]);

  const handleBookmarkJump = (chapterIndex: number, bookmark: Bookmark) => void jumpTo(chapterIndex, bookmark.anchor);
  const handleHistoryJump = (chapterIndex: number, entry: HistoryEntry) => void jumpTo(chapterIndex, entry.anchor);
  const handleSearchJump = (chapterIndex: number, position: number, length: number) => void jumpTo(chapterIndex, undefined, { position, length });
  const handleReturnToPrevious = () => {
    if (!returnAnchor) return;
    void jumpTo(returnAnchor.chapterIndex, returnAnchor.anchor);
    setReturnAnchor(null);
  };
  const handleApplyPreset = useCallback(
    (id: Parameters<typeof applyPreset>[0]) => {
      void applyPreset(id);
    },
    [applyPreset],
  );

  const handleApplySize = useCallback(
    (width: number, height: number) => {
      void applySize(width, height);
    },
    [applySize],
  );

  const handleUpdateWindowSettings = useCallback(
    (partial: Parameters<typeof updateWindowSettings>[0]) => {
      void updateWindowSettings(partial);
    },
    [updateWindowSettings],
  );

  useEffect(() => {
    if (!windowSettings.toolbar_auto_hide) {
      setToolbarHidden(false);
      return;
    }
    const onMouseMove = (event: MouseEvent) => {
      const nearTop = event.clientY <= 72;
      setToolbarHidden(!nearTop && !panelOpen);
    };
    window.addEventListener("mousemove", onMouseMove);
    return () => window.removeEventListener("mousemove", onMouseMove);
  }, [panelOpen, setToolbarHidden, windowSettings.toolbar_auto_hide]);

  useEffect(() => {
    if (!windowSettings.blur_curtain) return;
    const onBlur = () => setCurtain(true);
    window.addEventListener("blur", onBlur);
    return () => window.removeEventListener("blur", onBlur);
  }, [setCurtain, windowSettings.blur_curtain]);

  const handleHideWindow = useCallback(() => {
    commitProgress(layout.snapshot());
    windowApi.hide().catch((reason) => console.error("隐藏窗口失败:", reason));
  }, [commitProgress, layout]);

  const chapterProgress = useMemo(() => {
    if (!currentChapter) return 0;
    const ratio =
      mode === "paged"
        ? layout.pageCount <= 1
          ? 0
          : layout.page / (layout.pageCount - 1)
        : layoutInfo.ratio;
    return estimateBookProgress(currentChapter.index, chapterCount, ratio);
  }, [chapterCount, currentChapter, layout.page, layout.pageCount, layoutInfo.ratio, mode]);

  const backgroundCss = lowDistraction ? null : backgroundImageCss(settings.custom_bg_image);

  return (
    <div className={"reader-page" + (narrow ? " is-narrow" : "")} ref={rootRef}>
      {backgroundCss && <div className="reader-bg-overlay" style={{ backgroundImage: backgroundCss ?? undefined }} />}

      <ReaderToolbar
        bookTitle={bookTitle}
        chapterTitle={currentChapter?.title ?? ""}
        chapterIndex={currentChapter?.index ?? 0}
        chapterCount={chapterCount}
        mode={mode}
        narrow={narrow}
        lowDistraction={lowDistraction}
        sidebarVisible={sidebarOpen}
        panel={panel}
        hidden={toolbarHidden}
        onBack={handleBackToShelf}
        onToggleSidebar={toggleSidebar}
        onOpenPanel={openPanel}
        onToggleMode={handleToggleMode}
        onToggleLowDistraction={() => updateWindowSettings({ low_distraction: !lowDistraction })}
        alwaysOnTop={windowSettings.always_on_top}
        onToggleAlwaysOnTop={() =>
          updateWindowSettings({ always_on_top: !windowSettings.always_on_top })
        }
        onCurtain={() => setCurtain(true)}
        onHideWindow={handleHideWindow}
        onRestoreToolbar={() => setToolbarHidden(false)}
        onAddBookmark={() => void handleAddBookmark()}
        onOpenBookmarks={() => openPanel("bookmarks")}
        onOpenHistory={() => openPanel("history")}
        hasBookmarkHere={hasBookmarkHere}
      />

      <div className="reader-body">
        {sidebarOpen && (
          <ReaderSidebar
            bookTitle={bookTitle}
            chapterCount={chapterCount}
            toc={toc}
            currentIndex={currentChapter?.index ?? -1}
            overlay={narrow}
            onSelect={(index) => {
              void jumpTo(index);
              if (narrow) closeSidebar();
            }}
            onClose={closeSidebar}
            onPrevChapter={() => currentChapter && void jumpTo(currentChapter.index - 1)}
            onNextChapter={() => currentChapter && void jumpTo(currentChapter.index + 1)}
          />
        )}

        <div className={"reader-main reader-mode-" + mode}>
          <div className="reader-content" ref={containerRef}>
            {currentChapter ? (
              <div
                ref={trackRef}
                className="reader-track"
                dangerouslySetInnerHTML={{ __html: currentChapter.content }}
              />
            ) : (
              <div className="reader-placeholder">
                <Icon name="book" size={28} />
                <span>{error ? "章节加载失败" : "正在准备这本书…"}</span>
              </div>
            )}

            {loading && currentChapter && (
              <div className="reader-loading" role="status" aria-live="polite">
                <div className="spinner" />
                正在加载章节…
              </div>
            )}

            {error && (
              <div className="reader-error" role="alert">
                <span>{error}</span>
                <button
                  className="btn btn-secondary"
                  onClick={() => void loadChapter(loadingChapter ?? currentChapter?.index ?? 0)}
                >
                  重试
                </button>
              </div>
            )}

            {panel === "search" && (
              <SearchPanel onClose={closePanel} onJump={handleSearchJump} />
            )}

            {panel === "bookmarks" && bookId && (
              <BookmarksPanel
                bookId={bookId}
                onClose={closePanel}
                onJump={handleBookmarkJump}
              />
            )}

            {panel === "history" && bookId && (
              <HistoryPanel
                bookId={bookId}
                onClose={closePanel}
                onJump={handleHistoryJump}
              />
            )}

            {panel === "reading" && (
              <ReadingSettingsPanel
                settings={settings}
                mode={mode}
                onClose={closePanel}
                onChange={(partial) => void updateSettings(partial)}
                onModeChange={(next) => void setReadingMode(next)}
              />
            )}

            {panel === "window" && (
              <WindowSizePanel
                settings={windowSettings}
                onClose={closePanel}
                onApplyPreset={handleApplyPreset}
                onApplySize={handleApplySize}
                onChange={handleUpdateWindowSettings}
              />
            )}
          </div>

          <footer className="reader-footer">
            <button
              className="icon-button"
              onClick={() => movePage(-1)}
              disabled={!currentChapter}
              aria-label="上一页"
              title={mode === "paged" ? "上一页（← / PageUp）" : "上一屏"}
            >
              <Icon name="chevron-left" size={18} />
            </button>

            <span className="footer-position">
              {mode === "paged" && layout.settled
                ? "第 " + (layout.page + 1) + " / " + layout.pageCount + " 页"
                : "滚动阅读"}
            </span>

            <button
              className="icon-button"
              onClick={() => movePage(1)}
              disabled={!currentChapter}
              aria-label="下一页"
              title={mode === "paged" ? "下一页（→ / PageDown）" : "下一屏"}
            >
              <Icon name="chevron-right" size={18} />
            </button>

            <div className="footer-spacer" />

            {returnAnchor && (
              <button className="footer-return" onClick={handleReturnToPrevious}>
                <Icon name="undo" size={14} />
                返回跳转前位置
              </button>
            )}

            <span className="footer-progress" title="按章节等权估算">
              <span className="progress-track">
                <span className="progress-fill" style={{ width: chapterProgress + "%" }} />
              </span>
              全书约 {chapterProgress}%
            </span>

            {!narrow && <span className="footer-hint">估算进度 · 章节等权</span>}
          </footer>
        </div>
      </div>

      {curtain && (
        <CurtainOverlay
          title={lowDistraction ? "随手记" : bookTitle || "轻阅"}
          onRestore={() => setCurtain(false)}
          onHideWindow={handleHideWindow}
        />
      )}

      {hotkeyNotice && (
        <div className="reader-toast" role="status" aria-live="polite">
          {hotkeyNotice}
        </div>
      )}
    </div>
  );
}
