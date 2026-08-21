import { useState, useCallback, useEffect, useRef } from "react";
import { useReaderStore } from "../store/reader";
import { ensureIndex, searchChapters, clearIndex } from "../lib/search";

interface SearchResult {
  chapterIndex: number;
  chapterTitle: string;
  text: string;
  position: number;
}

interface SearchPanelProps {
  onClose: () => void;
  onJump: (chapterIndex: number) => void;
}

const DISPLAY_LIMIT = 50;

export default function SearchPanel({ onClose, onJump }: SearchPanelProps) {
  const { bookId, chapterCount } = useReaderStore();
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SearchResult[]>([]);
  const [searching, setSearching] = useState(false);
  const [searched, setSearched] = useState(false);
  const [indexProgress, setIndexProgress] = useState<{ current: number; total: number } | null>(null);
  const [indexReady, setIndexReady] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  useEffect(() => {
    return () => {
      clearIndex();
    };
  }, [bookId]);

  const buildSearchIndex = useCallback(async () => {
    if (!bookId || indexReady) return;
    setIndexProgress({ current: 0, total: chapterCount });
    await ensureIndex(bookId, chapterCount, (current, total) => {
      setIndexProgress({ current, total });
    });
    setIndexReady(true);
    setIndexProgress(null);
  }, [bookId, chapterCount, indexReady]);

  const handleSearch = useCallback(async () => {
    if (!query.trim() || !bookId) return;

    setSearching(true);
    setSearched(true);

    if (!indexReady) {
      await buildSearchIndex();
    }

    const searchResults = searchChapters(query);
    setResults(searchResults);
    setSearching(false);
  }, [query, bookId, indexReady, buildSearchIndex]);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") {
      handleSearch();
    }
    if (e.key === "Escape") {
      onClose();
    }
  };

  const highlightMatch = (text: string, matchQuery: string) => {
    if (!matchQuery) return text;
    const parts = text.split(new RegExp(`(${matchQuery.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")})`, "gi"));
    return parts.map((part, i) =>
      part.toLowerCase() === matchQuery.toLowerCase() ? (
        <mark key={i}>{part}</mark>
      ) : (
        part
      )
    );
  };

  return (
    <div className="search-panel">
      <div className="search-header">
        <input
          ref={inputRef}
          type="text"
          placeholder="在书中搜索..."
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={handleKeyDown}
          style={{ flex: 1 }}
        />
        <button onClick={handleSearch} className="btn btn-primary" style={{ padding: "6px 12px" }}>
          搜索
        </button>
        <button onClick={onClose} className="btn-icon">
          ✕
        </button>
      </div>

      <div className="search-results">
        {indexProgress && (
          <div style={{ padding: "12px 16px", textAlign: "center", color: "var(--text-secondary)" }}>
            <div style={{ fontSize: 13, marginBottom: 6 }}>正在建立搜索索引...</div>
            <div
              style={{
                height: 3,
                background: "var(--border-color)",
                borderRadius: 2,
                overflow: "hidden",
              }}
            >
              <div
                style={{
                  height: "100%",
                  width: `${Math.round((indexProgress.current / Math.max(1, indexProgress.total)) * 100)}%`,
                  background: "var(--accent)",
                  transition: "width 0.2s",
                }}
              />
            </div>
            <div style={{ fontSize: 11, color: "var(--text-muted)", marginTop: 4 }}>
              {indexProgress.current} / {indexProgress.total}
            </div>
          </div>
        )}

        {searching && !indexProgress && (
          <div style={{ padding: "16px", textAlign: "center", color: "var(--text-secondary)" }}>
            搜索中...
          </div>
        )}

        {searched && !searching && results.length === 0 && (
          <div style={{ padding: "16px", textAlign: "center", color: "var(--text-secondary)" }}>
            未找到结果
          </div>
        )}

        {!searching && results.length > 0 && (
          <div style={{ padding: "6px 16px", fontSize: 12, color: "var(--text-muted)" }}>
            找到 {results.length} 个结果
          </div>
        )}

        {results.slice(0, DISPLAY_LIMIT).map((result, i) => (
          <div
            key={i}
            className="search-result-item"
            onClick={() => onJump(result.chapterIndex)}
          >
            <div className="result-chapter">{result.chapterTitle}</div>
            <div className="result-text">{highlightMatch(result.text, query)}</div>
          </div>
        ))}

        {results.length > DISPLAY_LIMIT && (
          <div style={{ padding: "8px 16px", fontSize: 12, color: "var(--text-muted)", textAlign: "center" }}>
            仅显示前 {DISPLAY_LIMIT} 条结果
          </div>
        )}
      </div>
    </div>
  );
}
