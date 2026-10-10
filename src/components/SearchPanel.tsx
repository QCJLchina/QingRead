import { useCallback, useEffect, useRef, useState } from "react";
import { useReaderStore } from "../store/reader";
import {
  MAX_RESULTS,
  cancelIndexBuild,
  ensureIndex,
  pageSlice,
  searchChapters,
  searchPageCount,
} from "../lib/search";
import type { SearchResult } from "../lib/search";

interface SearchPanelProps {
  onClose: () => void;
  /** 跳到命中位置：章节 + 正文位置 + 命中长度 */
  onJump: (chapterIndex: number, position: number, length: number) => void;
}

export default function SearchPanel({ onClose, onJump }: SearchPanelProps) {
  const { bookId, chapterCount } = useReaderStore();
  const [query, setQuery] = useState("");
  const [searchedQuery, setSearchedQuery] = useState("");
  const [results, setResults] = useState<SearchResult[]>([]);
  const [page, setPage] = useState(1);
  const [truncated, setTruncated] = useState(false);
  const [searching, setSearching] = useState(false);
  const [searched, setSearched] = useState(false);
  const [indexProgress, setIndexProgress] = useState<{ current: number; total: number } | null>(null);
  const [failedChapters, setFailedChapters] = useState<number[]>([]);
  const [indexIncomplete, setIndexIncomplete] = useState(false);
  const [activeIndex, setActiveIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  // 搜索期间重复提交不重复建索引
  const buildingRef = useRef(false);
  const mountedRef = useRef(true);

  useEffect(() => {
    inputRef.current?.focus();
    mountedRef.current = true;
    return () => { mountedRef.current = false; cancelIndexBuild(); };
  }, []);

  const runSearch = useCallback(
    async (term: string) => {
      if (!bookId || !term.trim()) return;
      if (buildingRef.current) return;
      buildingRef.current = true;
      setSearching(true);
      setSearched(true);
      setIndexProgress({ current: 0, total: chapterCount });
      try {
        const result = await ensureIndex(bookId, chapterCount, (progress) => {
          if (mountedRef.current) setIndexProgress({ current: progress.current, total: progress.total });
        });
        if (!mountedRef.current) return;
        setFailedChapters(result.failedChapters);
        setIndexIncomplete(!result.complete);
        {
          const found = searchChapters(term.trim());
          setSearchedQuery(term.trim());
          setResults(found.results);
          setTruncated(found.truncated);
          setPage(1);
          setActiveIndex(0);
        }
      } finally {
        buildingRef.current = false;
        if (mountedRef.current) { setSearching(false); setIndexProgress(null); }
      }
    },
    [bookId, chapterCount],
  );

  const handleCancel = useCallback(() => {
    cancelIndexBuild();
  }, []);

  const handleKeyDown = (event: React.KeyboardEvent) => {
    if (event.key === "Enter") {
      void runSearch(query);
    }
    if (event.key === "Escape") {
      onClose();
    }
  };

  const highlightMatch = (text: string, matchQuery: string) => {
    if (!matchQuery) return text;
    const parts = text.split(new RegExp(`(${matchQuery.replace(/[.*+?${}()|[\]\\]/g, "\\$&")})`, "gi"));
    return parts.map((part, i) =>
      part.toLowerCase() === matchQuery.toLowerCase() ? (
        <mark key={i}>{part}</mark>
      ) : (
        part
      ),
    );
  };

  const pageCount = searchPageCount(results.length);
  const visible = pageSlice(results, page);

  const move = (delta: number) => {
    if (results.length === 0) return;
    const next = (activeIndex + delta + results.length) % results.length;
    setActiveIndex(next);
    setPage(Math.floor(next / 50) + 1);
    const item = results[next];
    onJump(item.chapterIndex, item.position, item.length);
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
        <button onClick={() => void runSearch(query)} className="btn btn-primary" style={{ padding: "6px 12px" }}>
          搜索
        </button>
        <button onClick={onClose} className="btn-icon" aria-label="关闭搜索">
          ✕
        </button>
      </div>

      <div className="search-results">
        {indexProgress && (
          <div style={{ padding: "12px 16px", textAlign: "center", color: "var(--text-secondary)" }}>
            <div style={{ fontSize: 13, marginBottom: 6 }}>
              正在建立搜索索引 {indexProgress.current} / {indexProgress.total}
            </div>
            <div style={{ height: 3, background: "var(--border-color)", borderRadius: 2, overflow: "hidden" }}>
              <div
                style={{
                  height: "100%",
                  width: `${Math.round((indexProgress.current / Math.max(1, indexProgress.total)) * 100)}%`,
                  background: "var(--accent)",
                  transition: "width 0.2s",
                }}
              />
            </div>
            <button className="btn btn-secondary" style={{ marginTop: 8 }} onClick={handleCancel}>
              取消
            </button>
          </div>
        )}

        {indexIncomplete && failedChapters.length > 0 && (
          <div className="notice" role="status">
            {failedChapters.length} 章未能索引，结果可能不完整。
            <button
              className="btn-quiet"
              onClick={() => void runSearch(query)}
              disabled={searching}
            >
              重试
            </button>
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
          <div className="search-status">
            <span>
              第 {activeIndex + 1} / {results.length} 条
              {truncated ? `（超过 ${MAX_RESULTS} 条，请缩小关键词）` : ""}
            </span>
            <span className="search-nav">
              <button className="btn-quiet" onClick={() => move(-1)}>
                上一处
              </button>
              <button className="btn-quiet" onClick={() => move(1)}>
                下一处
              </button>
            </span>
          </div>
        )}

        {visible.map((result, i) => {
          const absolute = (page - 1) * 50 + i;
          return (
            <button
              key={absolute}
              type="button"
              className={"search-result-item" + (absolute === activeIndex ? " is-active" : "")}
              onClick={() => {
                setActiveIndex(absolute);
                onJump(result.chapterIndex, result.position, result.length);
              }}
            >
              <span className="result-chapter">{result.chapterTitle}</span>
              <span className="result-text">{highlightMatch(result.text, searchedQuery)}</span>
            </button>
          );
        })}

        {pageCount > 1 && (
          <div className="search-pager">
            <button
              className="btn-quiet"
              disabled={page <= 1}
              onClick={() => setPage((p) => Math.max(1, p - 1))}
            >
              上一页
            </button>
            <span>
              {page} / {pageCount}
            </span>
            <button
              className="btn-quiet"
              disabled={page >= pageCount}
              onClick={() => setPage((p) => Math.min(pageCount, p + 1))}
            >
              下一页
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
