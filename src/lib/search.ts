import { invoke } from "@tauri-apps/api/core";
import FlexSearch from "flexsearch";

interface SearchResult {
  chapterIndex: number;
  chapterTitle: string;
  text: string;
  position: number;
}

interface IndexEntry {
  id: string;
  chapterIndex: number;
  chapterTitle: string;
  text: string;
}

const MAX_RESULTS = 200;
const SNIPPET_LENGTH = 60;
const CONCURRENCY = 4; // Limit parallel IPC calls to avoid overwhelming Rust backend

let searchIndex: FlexSearch.Index | null = null;
let indexedBookId: string | null = null;
let indexedChapterCount = 0;
let indexData: Map<string, IndexEntry> = new Map();

export function stripHtml(html: string): string {
  return html.replace(/<[^>]*>/g, " ").replace(/\s+/g, " ").trim();
}

function createIndex(): FlexSearch.Index {
  return FlexSearch.create({
    tokenize: "forward",
  });
}

async function runWithConcurrency<T>(
  items: number[],
  concurrency: number,
  task: (item: number) => Promise<T>
): Promise<T[]> {
  const results: T[] = [];
  const executing = new Set<Promise<void>>();

  for (const item of items) {
    const promise = task(item).then((result) => {
      results.push(result);
    });
    executing.add(promise);

    if (executing.size >= concurrency) {
      await Promise.race(executing);
    }

    promise.finally(() => executing.delete(promise));
  }

  await Promise.all(executing);
  return results;
}

export async function ensureIndex(
  bookId: string,
  chapterCount: number,
  onProgress?: (current: number, total: number) => void
): Promise<void> {
  if (indexedBookId === bookId && indexedChapterCount >= chapterCount) {
    return;
  }

  searchIndex = createIndex();
  indexData = new Map();
  indexedBookId = bookId;
  indexedChapterCount = 0;

  let completedCount = 0;
  const reportProgress = () => {
    completedCount++;
    onProgress?.(completedCount, chapterCount);
  };

  const indices = Array.from({ length: chapterCount }, (_, i) => i);

  await runWithConcurrency(
    indices,
    CONCURRENCY,
    async (chapterIndex) => {
      try {
        const chapter = await invoke<{ index: number; title: string; content: string }>(
          "load_chapter",
          { bookId, chapterIndex }
        );

        const plainText = stripHtml(chapter.content);
        const entryId = `ch-${chapterIndex}`;

        searchIndex?.add(chapterIndex, plainText);

        indexData.set(entryId, {
          id: entryId,
          chapterIndex,
          chapterTitle: chapter.title,
          text: plainText,
        });

        indexedChapterCount = Math.max(indexedChapterCount, chapterIndex + 1);
      } catch {
        // Skip chapters that fail to load
      } finally {
        reportProgress();
      }
    }
  );
}

export function searchChapters(query: string): SearchResult[] {
  if (!searchIndex || !query.trim()) return [];

  const results = searchIndex.search(query.toLowerCase(), {
    limit: MAX_RESULTS,
  });

  const searchResults: SearchResult[] = [];

  for (const resultId of results) {
    const entryId = `ch-${resultId}`;
    const entry = indexData.get(entryId);
    if (!entry) continue;

    const text = entry.text;
    const lowerText = text.toLowerCase();
    const lowerQuery = query.toLowerCase();
    const position = lowerText.indexOf(lowerQuery);

    if (position === -1) continue;

    const start = Math.max(0, position - SNIPPET_LENGTH);
    const end = Math.min(text.length, position + query.length + SNIPPET_LENGTH);
    let snippet = text.slice(start, end);
    if (start > 0) snippet = "..." + snippet;
    if (end < text.length) snippet = snippet + "...";

    searchResults.push({
      chapterIndex: entry.chapterIndex,
      chapterTitle: entry.chapterTitle,
      text: snippet,
      position,
    });

    if (searchResults.length >= MAX_RESULTS) break;
  }

  return searchResults;
}

export function clearIndex(): void {
  searchIndex = null;
  indexedBookId = null;
  indexedChapterCount = 0;
  indexData = new Map();
}
