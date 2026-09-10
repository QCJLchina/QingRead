import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import BookCard from "../components/BookCard";
import BookImporter from "../components/BookImporter";
import BookLogo from "../components/BookLogo";
import Icon from "../components/icons";
import { useLibraryStore } from "../store/library";

type LibraryView = "grid" | "list";
const VIEW_KEY = "qingread.library.view";

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

  useEffect(() => {
    void loadBooks();
  }, [loadBooks]);

  useEffect(() => {
    window.localStorage.setItem(VIEW_KEY, view);
  }, [view]);

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
              <button className="btn btn-secondary" onClick={selectAll}>全选</button>
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
          <span className="library-count">{books.length} 本</span>
        </div>
      </header>

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
      ) : (
        <>
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

          {view === "grid" ? (
            <div className="book-grid">
              {books.map((book) => (
                <BookCard key={book.id} book={book} selectionMode={selectionMode} view="grid" />
              ))}
            </div>
          ) : (
            <div className="book-list">
              {books.map((book) => (
                <BookCard key={book.id} book={book} selectionMode={selectionMode} view="list" />
              ))}
            </div>
          )}
        </>
      )}
    </div>
  );
}
