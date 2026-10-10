import { useEffect, useMemo, useState } from "react";
import { useReadingStore } from "../store/reading";
import type { HistoryEntry } from "../types";

interface HistoryPanelProps {
  bookId: string;
  onClose: () => void;
  onJump: (chapterIndex: number, entry: HistoryEntry) => void;
}

const KIND_LABEL: Record<string, string> = {
  open: "打开书籍",
  leave: "离开阅读",
  jump: "跳转",
};

function formatTime(seconds: number): string {
  if (!seconds) return "";
  const date = new Date(seconds * 1000);
  const now = Date.now() / 1000;
  const diff = now - seconds;
  if (diff < 60) return "刚刚";
  if (diff < 3600) return Math.floor(diff / 60) + " 分钟前";
  if (diff < 86400) return Math.floor(diff / 3600) + " 小时前";
  if (diff < 86400 * 7) return Math.floor(diff / 86400) + " 天前";
  return date.toLocaleDateString("zh-CN", { month: "numeric", day: "numeric" });
}

export default function HistoryPanel({ bookId, onClose, onJump }: HistoryPanelProps) {
  const data = useReadingStore((state) => state.data[bookId]);
  const clearHistory = useReadingStore((state) => state.clearHistory);
  const loadBook = useReadingStore((state) => state.loadBook);
  const [confirming, setConfirming] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const saveError = useReadingStore((state) => state.error);

  useEffect(() => {
    void loadBook(bookId).catch((error) => setLoadError(String(error)));
  }, [bookId, loadBook]);

  const entries = useMemo(() => {
    const list = [...(data?.history ?? [])];
    list.sort((a, b) => b.created_at - a.created_at);
    return list;
  }, [data?.history]);

  return (
    <div className="search-panel">
      <div className="search-header">
        <strong style={{ flex: 1 }}>阅读历史（{entries.length}）</strong>
        {entries.length > 0 && (
          <button
            className="btn btn-secondary danger"
            onClick={() => {
              if (!confirming) {
                setConfirming(true);
                return;
              }
              void clearHistory(bookId);
              setConfirming(false);
            }}
          >
            {confirming ? "确认清空？" : "清空"}
          </button>
        )}
        <button onClick={onClose} className="btn-icon" aria-label="关闭">
          ✕
        </button>
      </div>

      <div className="search-results">
        {(loadError || saveError) && <div role="alert" className="notice notice-error">{loadError || saveError}</div>}
        {entries.length === 0 ? (
          <div className="empty-panel">
            还没有阅读历史。打开书籍、离开阅读或跳转位置时会自动记录。
          </div>
        ) : (
          entries.map((entry) => (
            <button
              key={entry.id}
              type="button"
              className="search-result-item"
              onClick={() => onJump(entry.chapter_index, entry)}
            >
              <span className="result-chapter">
                {KIND_LABEL[entry.kind] ?? entry.kind} · 第 {entry.chapter_index + 1} 章
                {entry.created_at ? " · " + formatTime(entry.created_at) : ""}
              </span>
              <span className="result-text">{entry.snippet || "（无摘要）"}</span>
            </button>
          ))
        )}
      </div>
    </div>
  );
}
