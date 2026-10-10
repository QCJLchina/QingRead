import { invoke } from "@tauri-apps/api/core";
import { plainTextFromHtml } from "./text-model.ts";

export interface SearchResult {
  chapterIndex: number;
  chapterTitle: string;
  /** 展示用片段（含上下文，可能带前后省略号） */
  text: string;
  /** 命中在正文纯文本里的标准字符位置 */
  position: number;
  /** 命中长度，正文定位和高亮需要 */
  length: number;
}

interface IndexedChapter {
  chapterIndex: number;
  chapterTitle: string;
  text: string;
}

export const MAX_RESULTS = 200;
const PAGE_SIZE = 50;
const SNIPPET_LENGTH = 60;
const CONCURRENCY = 4;

let indexedBookId: string | null = null;
let chapters = new Map<number, IndexedChapter>();
/** 取消标记：变化后旧任务的返回会被丢弃 */
let cancelToken = 0;

export interface IndexProgress {
  current: number;
  total: number;
}

export interface IndexResult {
  cancelled: boolean;
  failedChapters: number[];
  complete: boolean;
}

export const stripHtml = plainTextFromHtml;

export function pageSlice<T>(items: T[], page: number): T[] {
  const start = (page - 1) * PAGE_SIZE;
  return items.slice(start, start + PAGE_SIZE);
}

export function searchPageCount(total: number): number {
  return Math.max(1, Math.ceil(total / PAGE_SIZE));
}

export const SEARCH_PAGE_SIZE = PAGE_SIZE;

/** 已索引的章节数，用于取消后续建。 */
export function resumeCount(): number {
  return chapters.size;
}

/**
 * 构建全书索引。
 * - 取消：停止调度后续章节，已完成部分保留，再次搜索时续建；
 * - 章节读取失败：跳过并在结果里报告，不标记为完整索引；
 * - 切书：cancelToken 变化后，旧请求的返回被丢弃。
 */
export async function ensureIndex(
  bookId: string,
  chapterCount: number,
  onProgress?: (progress: IndexProgress) => void,
): Promise<IndexResult> {
  if (indexedBookId !== bookId) {
    indexedBookId = bookId;
    chapters = new Map();
  }

  const token = ++cancelToken;
  const failedChapters: number[] = [];
  let processed = chapters.size;

  // 跳过已建好的章节，支持取消后续建
  const queue: number[] = [];
  for (let index = 0; index < chapterCount; index += 1) {
    if (!chapters.has(index)) queue.push(index);
  }

  const report = () => onProgress?.({ current: processed, total: chapterCount });
  report();

  const workers: Promise<void>[] = [];
  const workerCount = Math.min(CONCURRENCY, queue.length);
  for (let worker = 0; worker < workerCount; worker += 1) {
    workers.push(
      (async () => {
        for (;;) {
          if (token !== cancelToken) return;
          const chapterIndex = queue.shift();
          if (chapterIndex === undefined) return;
          try {
            const chapter = await invoke<{ index: number; title: string; content: string }>(
              "load_chapter",
              { bookId, chapterIndex },
            );
            if (token !== cancelToken) return;
            chapters.set(chapterIndex, {
              chapterIndex,
              chapterTitle: chapter.title,
              text: stripHtml(chapter.content),
            });
          } catch {
            failedChapters.push(chapterIndex);
          } finally {
            processed += 1;
            report();
          }
        }
      })(),
    );
  }

  await Promise.all(workers);

  const cancelled = token !== cancelToken;
  return {
    cancelled,
    failedChapters,
    complete: !cancelled && chapters.size >= chapterCount && failedChapters.length === 0,
  };
}

export function cancelIndexBuild(): void {
  cancelToken += 1;
}

/**
 * 在已索引正文里查找全部命中。
 * - 连续文本匹配，英文不区分大小写；
 * - 同一章节内的多次命中都返回；
 * - 按章节、位置排序；最多保留 MAX_RESULTS 条。
 */
export function searchChapters(query: string): { results: SearchResult[]; truncated: boolean } {
  if (!query.trim()) return { results: [], truncated: false };
  const needle = query.toLowerCase();
  const results: SearchResult[] = [];
  let truncated = false;

  const ordered = [...chapters.values()].sort((a, b) => a.chapterIndex - b.chapterIndex);
  for (const chapter of ordered) {
    const haystack = chapter.text.toLowerCase();
    let from = 0;
    for (;;) {
      const position = haystack.indexOf(needle, from);
      if (position === -1) break;
      from = position + Math.max(1, needle.length);

      if (results.length >= MAX_RESULTS) {
        truncated = true;
        break;
      }

      const start = Math.max(0, position - SNIPPET_LENGTH);
      const end = Math.min(chapter.text.length, position + needle.length + SNIPPET_LENGTH);
      let snippet = chapter.text.slice(start, end);
      if (start > 0) snippet = "..." + snippet;
      if (end < chapter.text.length) snippet = snippet + "...";

      results.push({
        chapterIndex: chapter.chapterIndex,
        chapterTitle: chapter.chapterTitle,
        text: snippet,
        position,
        length: query.length,
      });
    }
    if (truncated) break;
  }

  return { results, truncated };
}

export function clearIndex(): void {
  cancelIndexBuild();
  indexedBookId = null;
  chapters = new Map();
}
