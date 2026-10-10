import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import BookCard from "../components/BookCard";
import BookImporter from "../components/BookImporter";
import BookLogo from "../components/BookLogo";
import Icon from "../components/icons";
import { useLibraryStore } from "../store/library";
import { useReadingStore } from "../store/reading";
import { filterAndSort, statusLabel } from "../lib/library-filter";
import type { LibraryFilter, LibrarySort, ReadingStatus } from "../types";

type LibraryView = "grid" | "list";
const VIEW_KEY = "qingread.library.view";
const SORT_KEY = "qingread.library.sort";
const FILTER_KEY = "qingread.library.filter";

const SORTS: Array<{ id: LibrarySort; label: string }> = [
  { id: "recent", label: "最近阅读" },
  { id: "added", label: "添加时间" },
  { id: "title", label: "书名" },
];

const FILTERS: Array<{ id: LibraryFilter; label: string }> = [
  { id: "all", label: "全部" },
  { id: "unread", label: "未读" },
  { id: "reading", label: "在读" },
  { id: "finished", label: "已读" },
];

export default function LibraryPage() {
  const navigate = useNavigate();
  const {
    books,
    loading,
    error,
    selectedIds,
    recentBookId,
    progress,
    loadBooks,
    batchRemoveBooks,
    selectAll,
    clearSelection,
    clearError,
  } = useLibraryStore();

  const [selectionMode, setSelectionMode] = useState(false);
  const [view, setView] = useState<LibraryView>(() => {
    const stored = window.localStorage.getItem(VIEW_KEY);
    return stored === "list" ? "list" : "grid";
  });

  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<LibrarySort>(() => {
    const stored = window.localStorage.getItem(SORT_KEY);
    return stored === "added" || stored === "title" ? stored : "recent";
  });
  const [filter, setFilter] = useState<LibraryFilter>(() => {
    const stored = window.localStorage.getItem(FILTER_KEY);
    return stored === "unread" || stored === "reading" || stored === "finished" ? stored : "all";
  });
  const { summaries, loadSummaries, setStatus } = useReadingStore();

  useEffect(() => {
    void loadBooks();
  }, [loadBooks]);

  useEffect(() => {
    void loadSummaries();
  }, [loadSummaries]);

  useEffect(() => {
    window.localStorage.setItem(VIEW_KEY, view);
  }, [view]);

  useEffect(() => {
    window.localStorage.setItem(SORT_KEY, sort);
  }, [sort]);

  useEffect(() => {
    window.localStorage.setItem(FILTER_KEY, filter);
  }, [filter]);

  // 筛选条件变化时清空批量选择，避免选中已经不在列表里的书。
  useEffect(() => {
    clearSelection();
  }, [query, filter, sort, clearSelection]);

  const recentBook = useMemo(
    () => books.find((book) => book.id === recentBookId) ?? null,
    [books, recentBookId],
  );

  const recentEntry = recentBook ? progress[recentBook.id] : undefined;

  const handleBatchDelete = async () => {
    if (selectedIds.size === 0) return;
    if (confirm("确定要从书架移除选中的 " + selectedIds.size + " 本书吗？")) {
      await batchRemoveBooks(Array.from(selectedIds));
      setSelectionMode(false);
    }
  };

  const toggleSelectionMode = () => {
    if (selectionMode) {
      clearSelection();
      setSelectionMode(false);
      return;
    }
    setSelectionMode(true);
  };

  const visible = useMemo(
    () => filterAndSort(books, progress, summaries, query, filter, sort),
    [books, progress, summaries, query, filter, sort],
  );

  const handleStatusChange = async (bookId: string, status: ReadingStatus) => {
    await setStatus(bookId, status);
  };

  const hasConditions = query.trim().length > 0 || filter !== "all";
  const resetConditions = () => {
    setQuery("");
    setFilter("all");
  };

  return (
    <div className="library-page">
      <header className="library-header">
        <div>
          <div className="eyebrow">轻阅 · 书架</div>
          <h1>我的书架</h1>
        </div>
        <div className="library-actions">
          <div className="segmented">
            <button
              className={view === "grid" ? "is-active" : ""}
              aria-pressed={view === "grid"}
              onClick={() => setView("grid")}
              title="封面视图"
            >
              <Icon name="grid" size={15} />
            </button>
            <button
              className={view === "list" ? "is-active" : ""}
              aria-pressed={view === "list"}
              onClick={() => setView("list")}
              title="紧凑列表"
            >
              <Icon name="list" size={15} />
            </button>
          </div>
          {selectionMode && (
            <>
            <button
              className="btn btn-secondary"
              onClick={() => selectAll(visible.map((item) => item.book.id))}
            >
              全选
            </button>
              <button
                className="btn btn-secondary danger"
                disabled={selectedIds.size === 0}
                onClick={handleBatchDelete}
              >
                删除 ({selectedIds.size})
              </button>
            </>
          )}
          <button
            className={"btn " + (selectionMode ? "btn-primary" : "btn-secondary")}
            onClick={toggleSelectionMode}
          >
            {selectionMode ? "取消" : "批量管理"}
          </button>
          <span className="library-count">
            {hasConditions ? visible.length + " / " + books.length + " 本" : books.length + " 本"}
          </span>
        </div>
      </header>

      {books.length > 0 && (
        <div className="library-toolbar">
          <input
            type="search"
            className="library-search"
            placeholder="搜索书名或作者"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            aria-label="搜索书名或作者"
          />
          <div className="segmented">
            {FILTERS.map((item) => (
              <button
                key={item.id}
                className={filter === item.id ? "is-active" : ""}
                aria-pressed={filter === item.id}
                onClick={() => setFilter(item.id)}
              >
                {item.label}
              </button>
            ))}
          </div>
          <label className="library-sort">
            <span>排序</span>
            <select
              value={sort}
              onChange={(event) => setSort(event.target.value as LibrarySort)}
              aria-label="排序方式"
            >
              {SORTS.map((item) => (
                <option key={item.id} value={item.id}>
                  {item.label}
                </option>
              ))}
            </select>
          </label>
        </div>
      )}

          {recentBook && !selectionMode && (
            <section className="resume-card">
              <div className="resume-main">
                <div className="eyebrow">继续阅读</div>
                <strong>{recentBook.title}</strong>
                <span className="resume-meta">
                  {recentEntry ? "读到第 " + (recentEntry.chapter_index + 1) + " 章" : "刚刚打开过"}
                </span>
              </div>
              <button
                className="btn btn-primary"
                onClick={() => navigate("/reader/" + recentBook.id)}
              >
                继续阅读
                <Icon name="chevron-right" size={15} />
              </button>
            </section>
          )}
      <BookImporter />

      {error && (
        <div className="notice notice-error" onClick={clearError} role="alert">
          {error}
          <span className="notice-hint">（点击关闭）</span>
        </div>
      )}

      {loading && books.length === 0 ? (
        <div className="empty-state">
          <div className="spinner" />
          <div className="text">加载书架中…</div>
        </div>
      ) : books.length === 0 ? (
        <div className="empty-state">
          <BookLogo size={88} />
          <div className="title">书架是空的</div>
          <div className="text">支持 EPUB 和 TXT。拖到上方区域、点击浏览文件，或用命令行：</div>
          <code className="empty-code">qingread.exe "D:\books\novel.epub"</code>
        </div>
      ) : visible.length === 0 ? (
        <div className="empty-state">
          <Icon name="search" size={36} />
          <div className="title">没有符合条件的书</div>
          <div className="text">调整搜索词或筛选条件后再试。</div>
          {hasConditions && (
            <button className="btn btn-secondary" onClick={resetConditions}>
              清空条件
            </button>
          )}
        </div>
      ) : (
        <>


          {view === "grid" ? (
            <div className="book-grid">
              {visible.map((item) => (
                <BookCard
                  key={item.book.id}
                  book={item.book}
                  selectionMode={selectionMode}
                  view="grid"
                  status={item.status}
                  onStatusChange={handleStatusChange}
                />
              ))}
            </div>
          ) : (
            <div className="book-list">
              {visible.map((item) => (
                <BookCard
                  key={item.book.id}
                  book={item.book}
                  selectionMode={selectionMode}
                  view="list"
                  status={item.status}
                  onStatusChange={handleStatusChange}
                />
              ))}
            </div>
          )}
        </>
      )}
    </div>
  );
}
