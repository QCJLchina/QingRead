import { useEffect, useState } from "react";
import { useLibraryStore } from "../store/library";
import BookCard from "../components/BookCard";
import BookImporter from "../components/BookImporter";
import BookLogo from "../components/BookLogo";

export default function LibraryPage() {
  const {
    books,
    loading,
    error,
    selectedIds,
    loadBooks,
    batchRemoveBooks,
    selectAll,
    clearSelection,
  } = useLibraryStore();
  const [selectionMode, setSelectionMode] = useState(false);

  useEffect(() => {
    loadBooks();
  }, [loadBooks]);

  const handleBatchDelete = async () => {
    if (selectedIds.size === 0) return;
    if (confirm(`确定要从书架移除选中的 ${selectedIds.size} 本书吗？`)) {
      await batchRemoveBooks(Array.from(selectedIds));
      setSelectionMode(false);
    }
  };

  const handleToggleMode = () => {
    if (selectionMode) {
      clearSelection();
      setSelectionMode(false);
    } else {
      setSelectionMode(true);
    }
  };

  return (
    <div className="library-page">
      <div className="library-header">
        <h1>我的书架</h1>
        <div className="actions" style={{ display: "flex", gap: 8, alignItems: "center" }}>
          {selectionMode && (
            <>
              <button onClick={selectAll} className="btn btn-secondary" style={{ fontSize: 13, padding: "4px 12px" }}>
                全选
              </button>
              <button
                onClick={handleBatchDelete}
                className="btn btn-secondary"
                disabled={selectedIds.size === 0}
                style={{
                  fontSize: 13,
                  padding: "4px 12px",
                  color: selectedIds.size > 0 ? "#c33" : undefined,
                }}
              >
                删除 ({selectedIds.size})
              </button>
            </>
          )}
          <button
            onClick={handleToggleMode}
            className={selectionMode ? "btn btn-primary" : "btn btn-secondary"}
            style={{ fontSize: 13, padding: "4px 12px" }}
          >
            {selectionMode ? "取消" : "批量管理"}
          </button>
          <span style={{ fontSize: "14px", color: "var(--text-secondary)" }}>
            共 {books.length} 本书
          </span>
        </div>
      </div>

      <BookImporter />

      {error && (
        <div
          style={{
            padding: "12px 16px",
            background: "#fee",
            color: "#c33",
            borderRadius: "6px",
            marginBottom: "16px",
            fontSize: "14px",
          }}
        >
          {error}
        </div>
      )}

      {loading ? (
        <div className="empty-state">
            <div className="icon">⏳</div>
            <div className="text">加载书架中...</div>
        </div>
      ) : books.length === 0 ? (
        <div className="empty-state" style={{ padding: "48px 24px" }}>
          <div className="icon">
            <BookLogo size={96} />
          </div>
          <div className="title" style={{ fontSize: 20, marginTop: 16 }}>书架是空的</div>
          <div className="text" style={{ maxWidth: 360, lineHeight: 1.6, marginTop: 8 }}>
            支持 EPUB 和 TXT 格式。通过上方拖拽区域导入，或使用命令行：
          </div>
          <div
            style={{
              marginTop: 16,
              padding: "12px 20px",
              background: "var(--card-bg)",
              border: "1px solid var(--border-color)",
              borderRadius: 8,
              fontFamily: "'Courier New', monospace",
              fontSize: 13,
              color: "var(--text-secondary)",
              maxWidth: 420,
              wordBreak: "break-all",
            }}
          >
            epubreader.exe "D:\books\novel.epub"
          </div>
        </div>
      ) : (
        <div className="book-grid">
          {books.map((book) => (
            <BookCard key={book.id} book={book} selectionMode={selectionMode} />
          ))}
        </div>
      )}
    </div>
  );
}
