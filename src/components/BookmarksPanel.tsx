import { useEffect, useMemo, useState } from "react";
import { useReadingStore } from "../store/reading";
import type { Bookmark } from "../types";
import { compareBookmarks } from "../lib/bookmark-order";

interface BookmarksPanelProps {
  bookId: string;
  onClose: () => void;
  /** 跳到书签所在位置。anchor 为 null 时说明锚点已失效，需要提示用户。 */
  onJump: (chapterIndex: number, bookmark: Bookmark) => void;
}

type SortMode = "reading" | "created";

const NOTE_LIMIT = 500;

export default function BookmarksPanel({ bookId, onClose, onJump }: BookmarksPanelProps) {
  const data = useReadingStore((state) => state.data[bookId]);
  const saving = useReadingStore((state) => state.saving);
  const updateBookmark = useReadingStore((state) => state.updateBookmark);
  const removeBookmark = useReadingStore((state) => state.removeBookmark);
  const loadBook = useReadingStore((state) => state.loadBook);
  const [sortMode, setSortMode] = useState<SortMode>("reading");
  const [editing, setEditing] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const [error, setError] = useState<string | null>(null);
  const saveError = useReadingStore((state) => state.error);

  useEffect(() => {
    void loadBook(bookId).catch((error) => setError(String(error)));
  }, [bookId, loadBook]);

  const bookmarks = useMemo(() => {
    const list = [...(data?.bookmarks ?? [])];
    if (sortMode === "reading") {
      // 正文顺序：章节升序，同章按创建时间升序
      list.sort(compareBookmarks);
    } else {
      list.sort((a, b) => b.created_at - a.created_at);
    }
    return list;
  }, [data?.bookmarks, sortMode]);

  const startEdit = (bookmark: Bookmark) => {
    setEditing(bookmark.id);
    setDraft(bookmark.note ?? "");
    setError(null);
  };

  const saveEdit = async (bookmarkId: string) => {
    if (draft.length > NOTE_LIMIT) {
      setError("备注最多 " + NOTE_LIMIT + " 字");
      return;
    }
    try {
      await updateBookmark(bookId, bookmarkId, draft);
      setEditing(null);
      setDraft("");
    } catch {
      // 保存失败时保留编辑内容，允许重试
    }
  };

  const handleDelete = (bookmark: Bookmark) => {
    if (confirm("确定删除这个书签吗？")) {
      void removeBookmark(bookId, bookmark.id);
    }
  };

  return (
    <div className="search-panel">
      <div className="search-header">
        <strong style={{ flex: 1 }}>书签（{bookmarks.length}）</strong>
        <div className="segmented">
          <button
            className={sortMode === "reading" ? "is-active" : ""}
            onClick={() => setSortMode("reading")}
            title="按正文顺序"
          >
            正文顺序
          </button>
          <button
            className={sortMode === "created" ? "is-active" : ""}
            onClick={() => setSortMode("created")}
            title="按添加时间"
          >
            最新添加
          </button>
        </div>
        <button onClick={onClose} className="btn-icon" aria-label="关闭">
          ✕
        </button>
      </div>

      <div className="search-results">
        {(error || saveError) && (
          <div className="notice notice-error" onClick={() => setError(null)} role="alert">
            {error || saveError}
          </div>
        )}
        {bookmarks.length === 0 ? (
          <div className="empty-panel">还没有书签。阅读时点工具栏的书签按钮即可保存当前位置。</div>
        ) : (
          bookmarks.map((bookmark) => (
            <div key={bookmark.id} className="bookmark-item">
              {editing === bookmark.id ? (
                <div className="bookmark-edit">
                  <textarea
                    value={draft}
                    onChange={(event) => setDraft(event.target.value)}
                    placeholder="写点备注…"
                    maxLength={NOTE_LIMIT}
                    rows={3}
                    autoFocus
                  />
                  <div className="bookmark-edit-actions">
                    <button
                      className="btn btn-primary"
                      disabled={saving}
                      onClick={() => void saveEdit(bookmark.id)}
                    >
                      保存
                    </button>
                    <button className="btn btn-secondary" onClick={() => setEditing(null)}>
                      取消
                    </button>
                    <span className="hint">{draft.length} / {NOTE_LIMIT}</span>
                  </div>
                </div>
              ) : (
                <>
                  <button
                    type="button"
                    className="search-result-item"
                    onClick={() => onJump(bookmark.chapter_index, bookmark)}
                  >
                    <span className="result-chapter">第 {bookmark.chapter_index + 1} 章</span>
                    <span className="result-text">{bookmark.snippet || "（无摘要）"}</span>
                    {bookmark.note && <span className="result-note">{bookmark.note}</span>}
                  </button>
                  <div className="bookmark-actions">
                    <button className="btn-quiet" onClick={() => startEdit(bookmark)}>
                      备注
                    </button>
                    <button className="btn-quiet danger" onClick={() => handleDelete(bookmark)}>
                      删除
                    </button>
                  </div>
                </>
              )}
            </div>
          ))
        )}
      </div>
    </div>
  );
}
