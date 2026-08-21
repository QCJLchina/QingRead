import { useNavigate } from "react-router-dom";
import { useLibraryStore } from "../store/library";
import type { BookInfo } from "../types";
import BookLogo from "./BookLogo";

interface BookCardProps {
  book: BookInfo;
  selectionMode: boolean;
}

export default function BookCard({ book, selectionMode }: BookCardProps) {
  const navigate = useNavigate();
  const { removeBook, selectedIds, toggleSelect } = useLibraryStore();
  const isSelected = selectedIds.has(book.id);

  const handleClick = () => {
    if (selectionMode) {
      toggleSelect(book.id);
    } else {
      navigate(`/reader/${book.id}`);
    }
  };

  const handleRemove = async (e: React.MouseEvent) => {
    e.stopPropagation();
    if (confirm(`确定要从书架移除《${book.title}》吗？`)) {
      await removeBook(book.id);
    }
  };

  return (
    <div
      className={`book-card ${isSelected ? "selected" : ""}`}
      onClick={handleClick}
      style={{
        cursor: selectionMode ? "pointer" : "default",
        outline: isSelected ? "2px solid var(--accent)" : undefined,
        outlineOffset: 2,
      }}
    >
      {selectionMode && (
        <div
          style={{
            position: "absolute",
            top: 8,
            left: 8,
            width: 22,
            height: 22,
            borderRadius: 4,
            background: isSelected ? "var(--accent)" : "rgba(255,255,255,0.8)",
            border: `2px solid ${isSelected ? "var(--accent)" : "var(--border-color)"}`,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            zIndex: 2,
            fontSize: 12,
            color: isSelected ? "white" : "transparent",
          }}
        >
          ✓
        </div>
      )}
      <div className="cover">
        {book.cover ? (
          <img src={book.cover} alt={book.title} loading="lazy" />
        ) : (
          <div className="placeholder">
            <BookLogo size={72} format={book.format} />
          </div>
        )}
        <span
          style={{
            position: "absolute",
            top: 8,
            right: 8,
            padding: "2px 8px",
            background: book.format === "txt" ? "rgba(59,130,246,0.85)" : "rgba(0,0,0,0.5)",
            color: "white",
            borderRadius: 4,
            fontSize: 10,
            fontWeight: 600,
            letterSpacing: 0.5,
          }}
        >
          {book.format?.toUpperCase() || "EPUB"}
        </span>
      </div>
      <div className="info">
        <div className="title">{book.title}</div>
        <div className="author">{book.author || "未知作者"}</div>
      </div>
      {!selectionMode && (
        <div className="actions">
          <button onClick={handleClick}>阅读</button>
          <button onClick={handleRemove} className="delete">
            移除
          </button>
        </div>
      )}
    </div>
  );
}
