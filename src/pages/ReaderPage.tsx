import { useEffect, useRef, useState, useCallback } from "react";
import { useParams, useNavigate } from "react-router-dom";
import { listen } from "@tauri-apps/api/event";
import { useReaderStore } from "../store/reader";
import { backgroundImageCss, useSettingsStore } from "../store/settings";
import ChapterList from "../components/ChapterList";
import SearchPanel from "../components/SearchPanel";
import BookLogo from "../components/BookLogo";

interface LoadProgress {
  stage?: string;
  message?: string;
  current: number;
  total: number;
}

export default function ReaderPage() {
  const { bookId } = useParams<{ bookId: string }>();
  const navigate = useNavigate();
  const {
    chapterCount,
    currentChapter,
    chapterOffsets,
    progress,
    loading,
    openBook,
    loadChapter,
    preloadChapter,
    saveProgress,
    clearReader,
  } = useReaderStore();
  const { settings, updateSettings } = useSettingsStore();

  const [sidebarOpen, setSidebarOpen] = useState(true);
  const [searchOpen, setSearchOpen] = useState(false);
  const [progressInfo, setProgressInfo] = useState<LoadProgress | null>(null);
  const trackRef = useRef<HTMLDivElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  // 累计滚动位置（用于计算页码）
  const [scrollPos, setScrollPos] = useState(0);
  // 单页滚动高度（px），由容器高度和行高估算
  const [pageHeightPx, setPageHeightPx] = useState(1);

  // 监听加载进度
  useEffect(() => {
    const unlistenPromise = listen<LoadProgress>("load-progress", (event) => {
      setProgressInfo(event.payload);
    });
    return () => {
      unlistenPromise.then((unlisten) => unlisten());
    };
  }, []);

  // 加载完成 200ms 后清除进度
  useEffect(() => {
    if (!loading && progressInfo && progressInfo.current >= progressInfo.total) {
      const timer = setTimeout(() => setProgressInfo(null), 200);
      return () => clearTimeout(timer);
    }
  }, [loading, progressInfo]);

  // 打开书
  useEffect(() => {
    if (bookId) {
      openBook(bookId);
    }
    return () => {
      clearReader();
    };
  }, [bookId, openBook, clearReader]);

  // 章节切换时：把 reader-content 滚到顶部。
  // 注意：这里只在 currentChapter 真正切换时跑一次，
  // 之后不再重置滚动位置，避免读同一章时干扰。
  const lastChapterIndexRef = useRef<number | null>(null);
  const pendingRestorePageRef = useRef<number | null>(null);
  useEffect(() => {
    const idx = currentChapter?.index ?? null;
    if (idx === null) {
      lastChapterIndexRef.current = null;
      pendingRestorePageRef.current = null;
      return;
    }
    if (lastChapterIndexRef.current !== idx) {
      lastChapterIndexRef.current = idx;
      if (contentRef.current) {
        contentRef.current.scrollTop = 0;
      }
      setScrollPos(0);
      const resumePage =
        progress && progress.chapter_index === idx ? progress.page_in_chapter : 0;
      pendingRestorePageRef.current = resumePage > 0 ? resumePage : null;
    }
  }, [currentChapter, progress]);

  // 估算单页滚动高度
  useEffect(() => {
    if (!contentRef.current || !trackRef.current) return;
    const calc = () => {
      const c = contentRef.current;
      if (!c) return;
      const height = c.clientHeight - 80; // 减去 padding
      const computed = window.getComputedStyle(trackRef.current!);
      const lineHeight = parseFloat(computed.lineHeight) || 28;
      const linesPerPage = Math.max(1, Math.floor(height / lineHeight));
      setPageHeightPx(Math.max(1, linesPerPage * lineHeight));
    };
    calc();
    window.addEventListener("resize", calc);
    return () => window.removeEventListener("resize", calc);
  }, [settings.font_size, settings.line_height, currentChapter]);

  // 打开书时恢复上次阅读位置；等 pageHeightPx 真正算出后再滚动。
  useEffect(() => {
    if (pendingRestorePageRef.current === null || pageHeightPx <= 1) return;
    const el = contentRef.current;
    if (!el) return;
    const target = pendingRestorePageRef.current * pageHeightPx;
    const max = Math.max(0, el.scrollHeight - el.clientHeight);
    const next = Math.min(target, max);
    el.scrollTop = next;
    setScrollPos(next);
    pendingRestorePageRef.current = null;
  }, [pageHeightPx, currentChapter]);

  // 监听滚动，更新当前"页"用于显示。使用 RAF 避免高频 setState。
  const rafIdRef = useRef<number | null>(null);
  const handleScroll = useCallback(() => {
    if (!contentRef.current) return;
    if (rafIdRef.current !== null) return;

    rafIdRef.current = requestAnimationFrame(() => {
      rafIdRef.current = null;
      if (contentRef.current) {
        setScrollPos(contentRef.current.scrollTop);
      }
    });
  }, []);

  useEffect(() => {
    return () => {
      if (rafIdRef.current !== null) {
        cancelAnimationFrame(rafIdRef.current);
      }
    };
  }, []);

  // 手动接管滚轮：在挂载时挂一次（非 passive），之后不再解绑/重绑。
  // 用 ref 读最新 scrollTop，避免任何 React 状态变化干扰滚动位置。
  const contentRefForWheel = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    contentRefForWheel.current = contentRef.current;
    const el = contentRef.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      const eventTarget = e.target as HTMLElement | null;
      if (eventTarget?.closest(".search-panel")) return;
      e.preventDefault();
      e.stopPropagation();
      const target = contentRefForWheel.current;
      if (!target) return;
      const max = target.scrollHeight - target.clientHeight;
      const next = Math.max(0, Math.min(max, target.scrollTop + e.deltaY));
      if (next !== target.scrollTop) {
        target.scrollTop = next;
      }
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => {
      el.removeEventListener("wheel", onWheel);
    };
  }, []);

  // 计算累计页号（粗略）
  const baseOffset = currentChapter && chapterOffsets[currentChapter.index] !== undefined
    ? chapterOffsets[currentChapter.index]
    : 0;
  const totalBookPages = chapterOffsets[chapterOffsets.length - 1] || 0;
  // 估算当前章节已滚动到的页
  const currentPageInChapter = Math.max(0, Math.floor(scrollPos / Math.max(1, pageHeightPx)));
  const cumulativePage = baseOffset + currentPageInChapter + 1;

  // 章节切换：预加载相邻章节（不依赖 scrollPos，避免每次滚动都触发）
  useEffect(() => {
    if (!currentChapter || !bookId) return;
    if (currentChapter.index > 0) {
      preloadChapter(currentChapter.index - 1);
    }
    if (currentChapter.index < chapterCount - 1) {
      preloadChapter(currentChapter.index + 1);
    }
  }, [currentChapter, bookId, chapterCount, preloadChapter]);

  // 滚动时保存进度：debounce 500ms，避免每次 wheel tick 都走 IPC/触发 store 更新
  useEffect(() => {
    if (!currentChapter || !bookId) return;
    const timer = setTimeout(() => {
      saveProgress(
        currentChapter.index,
        currentPageInChapter,
        baseOffset + currentPageInChapter + 1
      );
    }, 500);
    return () => clearTimeout(timer);
  }, [currentChapter, scrollPos, bookId, saveProgress, currentPageInChapter, baseOffset]);

  const handleChapterSelect = (index: number) => {
    loadChapter(index);
  };

  const handlePrevChapter = () => {
    if (currentChapter && currentChapter.index > 0) {
      loadChapter(currentChapter.index - 1);
    }
  };

  const handleNextChapter = () => {
    if (currentChapter && currentChapter.index < chapterCount - 1) {
      loadChapter(currentChapter.index + 1);
    }
  };

  // 键盘翻章（不是翻页，是翻章节）
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        navigate("/");
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [navigate]);

  const cycleTheme = () => {
    const themes: Array<"light" | "dark" | "sepia" | "green"> = ["light", "dark", "sepia", "green"];
    const currentIdx = themes.indexOf(settings.theme);
    const nextIdx = (currentIdx + 1) % themes.length;
    updateSettings({ theme: themes[nextIdx] });
  };

  return (
    <div className="reader-page">
      {settings.custom_bg_image && (
        <div
          className="reader-bg-overlay"
          style={{
            backgroundImage: backgroundImageCss(settings.custom_bg_image) ?? undefined,
          }}
        />
      )}

      <div className={`reader-sidebar ${sidebarOpen ? "" : "collapsed"}`}>
        <div className="sidebar-header">
          <h3>目录</h3>
          <button onClick={() => setSidebarOpen(false)} className="btn-icon">
            ✕
          </button>
        </div>
        <div className="sidebar-content">
          <ChapterList onSelect={handleChapterSelect} />
        </div>
      </div>

      <div className="reader-main">
        <div className="reader-toolbar">
          <div className="toolbar-left">
            <button onClick={() => navigate("/")} title="返回书架">
              ← 书架
            </button>
            <button
              onClick={() => setSidebarOpen(!sidebarOpen)}
              title="切换侧边栏"
            >
              ☰
            </button>
          </div>

          <div className="toolbar-center">
            <span>
              {currentChapter?.title || (loading ? "正在加载中，请稍后" : "请选择章节")}
            </span>
            {currentChapter && (
              <span>
                第 {(currentChapter.index ?? 0) + 1} / {chapterCount} 章
              </span>
            )}
          </div>

          <div className="toolbar-right">
            <button onClick={() => setSearchOpen(!searchOpen)} title="搜索">
              🔍
            </button>
            <button onClick={cycleTheme} title="切换主题">
              🎨
            </button>
            <button
              onClick={handlePrevChapter}
              disabled={!currentChapter || currentChapter.index === 0}
              title="上一章"
            >
              ◀
            </button>
            <button
              onClick={handleNextChapter}
              disabled={!currentChapter || currentChapter.index >= chapterCount - 1}
              title="下一章"
            >
              ▶
            </button>
          </div>
        </div>

        <div
          className="reader-content"
          ref={contentRef}
          onScroll={handleScroll}
        >
          {loading || progressInfo ? (
            <div className="loading-overlay">
              <div className="loading-icon">⏳</div>
              <div className="loading-text">正在加载中，请稍后</div>
              {progressInfo && (
                <>
                  <div className="loading-stage">
                    {progressInfo.message || progressInfo.stage || "处理中..."}
                  </div>
                  <div className="progress-container">
                    <div
                      className="progress-bar"
                      style={{ width: `${Math.min(100, Math.round((progressInfo.current / Math.max(1, progressInfo.total)) * 100))}%` }}
                    />
                  </div>
                  <div className="progress-text">
                    {progressInfo.current} / {progressInfo.total}
                  </div>
                </>
              )}
            </div>
          ) : currentChapter ? (
            <div
              ref={trackRef}
              className="reader-track scroll"
              dangerouslySetInnerHTML={{ __html: currentChapter.content }}
            />
          ) : (
            <div className="empty-state" style={{ height: "100%" }}>
              <BookLogo size={80} />
              <div className="text">请选择章节开始阅读</div>
            </div>
          )}

          {searchOpen && (
            <SearchPanel
              onClose={() => setSearchOpen(false)}
              onJump={handleChapterSelect}
            />
          )}
        </div>
      </div>
    </div>
  );
}
