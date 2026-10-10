import { useNavigate } from "react-router-dom";
import Icon from "./icons";
import { useLibraryStore } from "../store/library";
import { statusLabel } from "../lib/library-filter";
import type { BookInfo, ReadingStatus } from "../types";
import BookLogo from "./BookLogo";

interface BookCardProps {
  book: BookInfo;
  selectionMode: boolean;
  view: "grid" | "list";
  status: ReadingStatus;
  onStatusChange: (bookId: string, status: ReadingStatus) => void;
}

export default function BookCard({
  book,
  selectionMode,
  view,
  status,
  onStatusChange,
}: BookCardProps) {
  const navigate = useNavigate();
  const { removeBook, selectedIds, toggleSelect, progress } = useLibraryStore();
  const isSelected = selectedIds.has(book.id);
  const entry = progress[book.id];

  const open = () => {
    if (selectionMode) {
      toggleSelect(book.id);
      return;
    }
    navigate("/reader/" + book.id);
  };

  const handleRemove = async (event: React.MouseEvent) => {
    event.stopPropagation();
    if (confirm("确定要从书架移除《" + book.title + "》吗？\n原始文件不会被删除。")) {
      await removeBook(book.id);
    }
  };

  const progressLabel = entry ? "读到第 " + (entry.chapter_index + 1) + " 章" : "";

  // 点击状态徽标不会触发打开书籍
  const cycleStatus = (event: React.MouseEvent) => {
    event.stopPropagation();
    const next: ReadingStatus =
      status === "unread" ? "reading" : status === "reading" ? "finished" : "unread";
    onStatusChange(book.id, next);
  };

  const statusBadge = (
    <span
      className={"status-badge is-" + status}
      title={
        status === "finished"
          ? "已读（点击改为未读）"
          : status === "reading"
            ? "在读（点击标为已读）"
            : "未读（点击标为在读）"
      }
      onClick={cycleStatus}
      role="button"
      tabIndex={0}
      onKeyDown={(event) => {
        if (event.key === "Enter") cycleStatus(event as unknown as React.MouseEvent);
      }}
    >
      {statusLabel(status)}
    </span>
  );

  if (view === "list") {
    return (
      <div
        className={"book-row" + (isSelected ? " is-selected" : "")}
        onClick={open}
        role="button"
        tabIndex={0}
        onKeyDown={(event) => {
          if (event.key === "Enter") open();
        }}
      >
        {selectionMode && (
          <span className={"select-dot" + (isSelected ? " is-on" : "")} aria-hidden="true" />
        )}
        <div className="row-cover">
          {book.cover ? (
            <img src={book.cover} alt="" loading="lazy" />
          ) : (
            <BookLogo size={22} format={book.format} />
          )}
        </div>
        <div className="row-main">
          <span className="row-title">{book.title}</span>
          <span className="row-meta">
            {book.author || "未知作者"}
            {progressLabel ? " · " + progressLabel : ""}
          </span>
        </div>
        {!selectionMode && statusBadge}
        <span className="row-format">{book.format?.toUpperCase() ?? "EPUB"}</span>
        {!selectionMode && (
          <div className="row-actions">
            <button className="btn-quiet" onClick={open}>阅读</button>
            <button className="btn-quiet danger" onClick={handleRemove}>移除</button>
          </div>
        )}
      </div>
    );
  }

  return (
    <div
      className={"book-card" + (isSelected ? " is-selected" : "")}
      onClick={open}
      role="button"
      tabIndex={0}
      onKeyDown={(event) => {
        if (event.key === "Enter") open();
      }}
    >
      {selectionMode && (
        <span className={"select-dot select-dot-float" + (isSelected ? " is-on" : "")} aria-hidden="true" />
      )}
      <div className="cover">
        {book.cover ? (
          <img src={book.cover} alt="" loading="lazy" />
        ) : (
          <BookLogo size={64} format={book.format} />
        )}
        <span className="format-badge">{book.format?.toUpperCase() ?? "EPUB"}</span>
      </div>
      <div className="info">
        <span className="title">{book.title}</span>
        <span className="author">{book.author || "未知作者"}</span>
        <div className="info-meta">
          {progressLabel && <span className="progress-note">{progressLabel}</span>}
          {statusBadge}
        </div>
      </div>
      {!selectionMode && (
        <div className="card-actions">
          <button className="btn-quiet" onClick={open}>
            <Icon name="book" size={14} />
            阅读
          </button>
          <button className="btn-quiet danger" onClick={handleRemove}>
            <Icon name="close" size={14} />
          </button>
        </div>
      )}
    </div>
  );
}
