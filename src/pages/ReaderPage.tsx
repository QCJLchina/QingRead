import { useEffect, useRef, useState, useCallback } from "react";
import { useParams, useNavigate } from "react-router-dom";
import { listen } from "@tauri-apps/api/event";
import { useReaderStore } from "../store/reader";
import { backgroundImageCss, useSettingsStore } from "../store/settings";
import ChapterList from "../components/ChapterList";
import SearchPanel from "../components/SearchPanel";
import BookLogo from "../components/BookLogo";
import { calculateBookProgress, clampPage, pageFromRatio, ratioFromPage } from "../lib/pagination";

interface LoadProgress { stage?: string; message?: string; current: number; total: number; }
type ReaderMode = "scroll" | "paged";
const CONTENT_PADDING = 80;

export default function ReaderPage() {
  const { bookId } = useParams<{ bookId: string }>();
  const navigate = useNavigate();
  const { chapterCount, currentChapter, chapterOffsets, progress, loading, openBook, loadChapter, preloadChapter, saveProgress, clearReader } = useReaderStore();
  const { settings, updateSettings } = useSettingsStore();
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const [searchOpen, setSearchOpen] = useState(false);
  const [progressInfo, setProgressInfo] = useState<LoadProgress | null>(null);
  const [mode, setMode] = useState<ReaderMode>("scroll");
  const [scrollPos, setScrollPos] = useState(0);
  const [pageHeightPx, setPageHeightPx] = useState(1);
  const [currentPage, setCurrentPage] = useState(0);
  const [pageCount, setPageCount] = useState(1);
  const [pageReady, setPageReady] = useState(false);
  const contentRef = useRef<HTMLDivElement>(null);
  const trackRef = useRef<HTMLDivElement>(null);
  const currentPageRef = useRef(0);
  const pageCountRef = useRef(1);
  const pendingPageRef = useRef<number | null>(null);
  const pendingPageRatioRef = useRef<number | null>(null);
  const pendingScrollRatioRef = useRef<number | null>(null);
  const previousModeRef = useRef<ReaderMode>(mode);
  const layoutVersionRef = useRef(0);
  const reflowTimerRef = useRef<number | null>(null);
  const lastChapterRef = useRef<number | null>(null);
  const rafRef = useRef<number | null>(null);

  const setPage = useCallback((value: number) => {
    const page = clampPage(value, pageCountRef.current);
    currentPageRef.current = page;
    setCurrentPage(page);
  }, []);

  useEffect(() => {
    const unlisten = listen<LoadProgress>("load-progress", (event) => setProgressInfo(event.payload));
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  useEffect(() => {
    if (!loading && progressInfo && progressInfo.current >= progressInfo.total) {
      const timer = setTimeout(() => setProgressInfo(null), 200);
      return () => clearTimeout(timer);
    }
  }, [loading, progressInfo]);

  useEffect(() => {
    if (bookId) openBook(bookId);
    return () => clearReader();
  }, [bookId, openBook, clearReader]);

  useEffect(() => {
    const index = currentChapter?.index ?? null;
    if (index === null) { lastChapterRef.current = null; return; }
    if (lastChapterRef.current !== index) {
      lastChapterRef.current = index;
      const saved = progress?.chapter_index === index ? progress.page_in_chapter : 0;
      if (pendingPageRef.current === null) pendingPageRef.current = saved;
      setScrollPos(0);
      setPage(0);
      setPageCount(1);
      pageCountRef.current = 1;
      setPageReady(false);
      requestAnimationFrame(() => { if (contentRef.current) contentRef.current.scrollTop = 0; });
    }
  }, [currentChapter?.index, progress, setPage]);

  // 在两种阅读模式之间切换时，用当前位置比例换算，避免跳回旧的持久化页码。
  useEffect(() => {
    const previousMode = previousModeRef.current;
    if (previousMode === mode || !currentChapter || !contentRef.current) return;
    previousModeRef.current = mode;
    pendingPageRef.current = null;
    if (mode === "paged" && previousMode === "scroll") {
      const element = contentRef.current;
      const maxScroll = Math.max(0, element.scrollHeight - element.clientHeight);
      pendingPageRatioRef.current = maxScroll > 0 ? element.scrollTop / maxScroll : 0;
    } else if (mode === "scroll" && previousMode === "paged") {
      pendingScrollRatioRef.current = ratioFromPage(currentPageRef.current, pageCountRef.current);
    }
  }, [currentChapter, mode]);

  // Scroll mode: retain the existing continuous reading behavior and progress calculation.
  useEffect(() => {
    if (mode !== "scroll" || !contentRef.current || !trackRef.current) return;
    const calculate = () => {
      const container = contentRef.current;
      const track = trackRef.current;
      if (!container || !track) return;
      const height = container.clientHeight - CONTENT_PADDING;
      const lineHeight = parseFloat(getComputedStyle(track).lineHeight) || 28;
      setPageHeightPx(Math.max(1, Math.floor(Math.max(1, height) / lineHeight) * lineHeight));
    };
    calculate();
    window.addEventListener("resize", calculate);
    return () => window.removeEventListener("resize", calculate);
  }, [mode, currentChapter, settings.font_size, settings.line_height]);

  // 滚动模式继续恢复上次阅读位置；分页模式由 reflowPages 按实际页数恢复。
  useEffect(() => {
    if (mode !== "scroll" || !currentChapter || pageHeightPx <= 1) return;
    const savedPage = progress?.chapter_index === currentChapter.index ? progress.page_in_chapter : 0;
    const modeRatio = pendingScrollRatioRef.current;
    pendingScrollRatioRef.current = null;
    requestAnimationFrame(() => {
      const element = contentRef.current;
      if (!element) return;
      const maxScroll = Math.max(0, element.scrollHeight - element.clientHeight);
      const target = modeRatio === null
        ? Math.min(savedPage * pageHeightPx, maxScroll)
        : Math.round(modeRatio * maxScroll);
      element.scrollTop = target;
      setScrollPos(target);
    });
  }, [currentChapter, mode, pageHeightPx, progress]);

  useEffect(() => {
    if (mode !== "scroll" || !currentChapter || !bookId) return;
    const timer = setTimeout(() => {
      const base = chapterOffsets[currentChapter.index] ?? 0;
      const page = Math.max(0, Math.floor(scrollPos / Math.max(1, pageHeightPx)));
      saveProgress(currentChapter.index, page, base + page + 1);
    }, 500);
    return () => clearTimeout(timer);
  }, [bookId, chapterOffsets, currentChapter, mode, pageHeightPx, saveProgress, scrollPos]);

  const reflowPages = useCallback(async () => {
    if (mode !== "paged" || !currentChapter || !contentRef.current || !trackRef.current) return;
    const layoutVersion = ++layoutVersionRef.current;
    const container = contentRef.current;
    const track = trackRef.current;
    setPageReady(false);
    await Promise.all(Array.from(track.querySelectorAll("img")).map((image) => image.complete
      ? Promise.resolve()
      : new Promise<void>((resolve) => {
          image.addEventListener("load", () => resolve(), { once: true });
          image.addEventListener("error", () => resolve(), { once: true });
        })));
    if (layoutVersion !== layoutVersionRef.current) return;
    await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
    if (layoutVersion !== layoutVersionRef.current) return;
    const width = Math.max(1, container.clientWidth);
    track.style.setProperty("--reader-page-width", `${Math.max(1, width - CONTENT_PADDING)}px`);
    track.style.setProperty("--reader-page-height", `${Math.max(1, container.clientHeight - CONTENT_PADDING)}px`);
    await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
    if (layoutVersion !== layoutVersionRef.current) return;
    const count = Math.max(1, Math.ceil(track.scrollWidth / width));
    const requested = pendingPageRef.current;
    const requestedRatio = pendingPageRatioRef.current;
    pendingPageRef.current = null;
    pendingPageRatioRef.current = null;
    const target = requested === Number.MAX_SAFE_INTEGER
      ? count - 1
      : requestedRatio !== null
        ? pageFromRatio(requestedRatio, count)
        : requested ?? Math.min(currentPageRef.current, count - 1);
    pageCountRef.current = count;
    setPageCount(count);
    setPage(target);
    setPageReady(true);
    requestAnimationFrame(() => { if (contentRef.current) contentRef.current.scrollLeft = currentPageRef.current * width; });
  }, [currentChapter, mode, setPage]);

  useEffect(() => {
    if (mode !== "paged" || !currentChapter) return;
    const scheduleReflow = () => {
      if (reflowTimerRef.current !== null) window.clearTimeout(reflowTimerRef.current);
      reflowTimerRef.current = window.setTimeout(() => {
        reflowTimerRef.current = null;
        void reflowPages();
      }, 80);
    };
    const observer = new ResizeObserver(scheduleReflow);
    if (contentRef.current) observer.observe(contentRef.current);
    scheduleReflow();
    return () => {
      observer.disconnect();
      layoutVersionRef.current += 1;
      if (reflowTimerRef.current !== null) {
        window.clearTimeout(reflowTimerRef.current);
        reflowTimerRef.current = null;
      }
    };
  }, [currentChapter, mode, reflowPages, settings.font_family, settings.font_size, settings.line_height, sidebarOpen]);

  const movePage = useCallback((direction: -1 | 1) => {
    if (!currentChapter || !pageReady) return;
    const next = currentPageRef.current + direction;
    if (next < 0) {
      if (currentChapter.index > 0) { pendingPageRef.current = Number.MAX_SAFE_INTEGER; loadChapter(currentChapter.index - 1); }
    } else if (next >= pageCountRef.current) {
      if (currentChapter.index < chapterCount - 1) { pendingPageRef.current = 0; loadChapter(currentChapter.index + 1); }
    } else {
      setPage(next);
      if (contentRef.current) contentRef.current.scrollTo({ left: next * contentRef.current.clientWidth, behavior: "smooth" });
    }
  }, [chapterCount, currentChapter, loadChapter, pageReady, setPage]);

  useEffect(() => {
    const element = contentRef.current;
    if (!element) return;
    const onWheel = (event: WheelEvent) => {
      if (mode !== "paged") return;
      if ((event.target as HTMLElement | null)?.closest(".search-panel")) return;
      event.preventDefault();
      movePage((event.deltaX || event.deltaY) > 0 ? 1 : -1);
    };
    element.addEventListener("wheel", onWheel, { passive: false });
    return () => element.removeEventListener("wheel", onWheel);
  }, [mode, movePage]);

  const handleScroll = useCallback(() => {
    if (mode === "paged") {
      if (rafRef.current !== null) return;
      rafRef.current = requestAnimationFrame(() => {
        rafRef.current = null;
        if (contentRef.current?.clientWidth) setPage(Math.round(contentRef.current.scrollLeft / contentRef.current.clientWidth));
      });
    } else if (contentRef.current) {
      setScrollPos(contentRef.current.scrollTop);
    }
  }, [mode, setPage]);

  useEffect(() => () => { if (rafRef.current !== null) cancelAnimationFrame(rafRef.current); }, []);

  useEffect(() => {
    if (mode !== "paged" || !currentChapter || !bookId || !pageReady) return;
    const timer = setTimeout(() => {
      const base = chapterOffsets[currentChapter.index] ?? 0;
      saveProgress(currentChapter.index, currentPage, base + currentPage + 1);
    }, 500);
    return () => clearTimeout(timer);
  }, [bookId, chapterOffsets, currentChapter, currentPage, mode, pageReady, saveProgress]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (target?.matches("input, textarea, select, [contenteditable=true]")) return;
      if (event.key === "Escape") navigate("/");
      if (mode === "paged" && ["ArrowRight", "PageDown", " "].includes(event.key)) { event.preventDefault(); movePage(1); }
      if (mode === "paged" && ["ArrowLeft", "PageUp"].includes(event.key)) { event.preventDefault(); movePage(-1); }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [mode, movePage, navigate]);

  useEffect(() => {
    if (!currentChapter || !bookId) return;
    if (currentChapter.index > 0) preloadChapter(currentChapter.index - 1);
    if (currentChapter.index < chapterCount - 1) preloadChapter(currentChapter.index + 1);
  }, [bookId, chapterCount, currentChapter, preloadChapter]);

  const handleChapterSelect = (index: number) => { pendingPageRef.current = 0; loadChapter(index); };
  const handlePrevChapter = () => { if (currentChapter && currentChapter.index > 0) { pendingPageRef.current = Number.MAX_SAFE_INTEGER; loadChapter(currentChapter.index - 1); } };
  const handleNextChapter = () => { if (currentChapter && currentChapter.index < chapterCount - 1) { pendingPageRef.current = 0; loadChapter(currentChapter.index + 1); } };
  const cycleTheme = () => {
    const themes: Array<"light" | "dark" | "sepia" | "green"> = ["light", "dark", "sepia", "green"];
    const index = themes.indexOf(settings.theme);
    updateSettings({ theme: themes[(index + 1) % themes.length] });
  };
  const scrollPage = Math.max(0, Math.floor(scrollPos / Math.max(1, pageHeightPx)));
  const bookProgress = currentChapter
    ? mode === "paged"
      ? calculateBookProgress(currentChapter.index, chapterCount, currentPage, pageCount)
      : Math.min(100, Math.round(((currentChapter.index + scrollPage + 1) / Math.max(1, chapterCount)) * 100))
    : 0;

  return <div className="reader-page">
    {settings.custom_bg_image && <div className="reader-bg-overlay" style={{ backgroundImage: backgroundImageCss(settings.custom_bg_image) ?? undefined }} />}
    <div className={`reader-sidebar ${sidebarOpen ? "" : "collapsed"}`}>
      <div className="sidebar-header"><h3>目录</h3><button onClick={() => setSidebarOpen(false)} className="btn-icon">✕</button></div>
      <div className="sidebar-content"><ChapterList onSelect={handleChapterSelect} /></div>
    </div>
    <div className="reader-main">
      <div className="reader-toolbar">
        <div className="toolbar-left"><button onClick={() => navigate("/")}>← 书架</button><button onClick={() => setSidebarOpen(!sidebarOpen)}>☰</button></div>
        <div className="toolbar-center"><span>{currentChapter?.title || (loading ? "正在加载中，请稍后" : "请选择章节")}</span>{currentChapter && <span>第 {currentChapter.index + 1} / {chapterCount} 章</span>}</div>
        <div className="toolbar-right">
          <button onClick={() => setSearchOpen(!searchOpen)} title="搜索">🔍</button><button onClick={cycleTheme} title="切换主题">🎨</button>
          <button className={mode === "scroll" ? "active" : ""} onClick={() => setMode("scroll")} title="滚动阅读">☷</button>
          <button className={mode === "paged" ? "active" : ""} onClick={() => setMode("paged")} title="翻页阅读">▤</button>
          <button onClick={handlePrevChapter} disabled={!currentChapter || currentChapter.index === 0}>◀</button><button onClick={handleNextChapter} disabled={!currentChapter || currentChapter.index >= chapterCount - 1}>▶</button>
        </div>
      </div>
      <div className={`reader-content reader-mode-${mode}`} ref={contentRef} onScroll={handleScroll}>
        {loading || progressInfo ? <div className="loading-overlay"><div className="loading-icon">⏳</div><div className="loading-text">正在加载中，请稍后</div>{progressInfo && <><div className="loading-stage">{progressInfo.message || progressInfo.stage || "处理中..."}</div><div className="progress-container"><div className="progress-bar" style={{ width: `${Math.min(100, Math.round((progressInfo.current / Math.max(1, progressInfo.total)) * 100))}%` }} /></div><div className="progress-text">{progressInfo.current} / {progressInfo.total}</div></>}</div>
          : currentChapter ? <div ref={trackRef} className="reader-track" dangerouslySetInnerHTML={{ __html: currentChapter.content }} />
          : <div className="empty-state" style={{ height: "100%" }}><BookLogo size={80} /><div className="text">请选择章节开始阅读</div></div>}
        {mode === "paged" && currentChapter && pageReady && <><button className="page-zone page-zone-prev" onClick={() => movePage(-1)} aria-label="上一页" /><button className="page-zone page-zone-next" onClick={() => movePage(1)} aria-label="下一页" /></>}
        {searchOpen && <SearchPanel onClose={() => setSearchOpen(false)} onJump={handleChapterSelect} />}
      </div>
      {currentChapter && <div className="reader-pagination">
        {mode === "paged" ? <><button className="pagination-button" onClick={() => movePage(-1)} disabled={currentChapter.index === 0 && currentPage === 0}>‹</button><span>第 {currentPage + 1} / {pageCount} 页</span></> : <span>滚动阅读</span>}
        <span className="pagination-progress">全书进度 {bookProgress}%</span>
        {mode === "paged" ? <button className="pagination-button" onClick={() => movePage(1)} disabled={currentChapter.index === chapterCount - 1 && currentPage >= pageCount - 1}>›</button> : <span />}
      </div>}
    </div>
  </div>;
}
