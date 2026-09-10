import { create } from "zustand";
import { libraryApi, progressApi } from "../api/index.ts";
import type { ProgressSummary } from "../api/index.ts";
import type { BookInfo } from "../types";

interface LibraryState {
  books: BookInfo[];
  loading: boolean;
  error: string | null;
  selectedIds: Set<string>;
  /** book_id -> 最近一次阅读进度 */
  progress: Record<string, ProgressSummary>;
  /** 最近在读的书，用于书架顶部的「继续阅读」 */
  recentBookId: string | null;

  loadBooks: () => Promise<void>;
  importBook: (path: string) => Promise<BookInfo | null>;
  removeBook: (bookId: string) => Promise<void>;
  batchRemoveBooks: (bookIds: string[]) => Promise<void>;
  toggleSelect: (bookId: string) => void;
  selectAll: () => void;
  clearSelection: () => void;
  clearError: () => void;
}

function pickRecent(
  books: BookInfo[],
  progress: ProgressSummary[],
): { recentBookId: string | null; map: Record<string, ProgressSummary> } {
  const map: Record<string, ProgressSummary> = {};
  const ids = new Set(books.map((book) => book.id));
  for (const item of progress) {
    if (!ids.has(item.book_id)) continue;
    map[item.book_id] = item;
  }
  const candidates = Object.values(map).sort((a, b) => b.last_read - a.last_read);
  return { recentBookId: candidates.length > 0 ? candidates[0].book_id : null, map };
}

export const useLibraryStore = create<LibraryState>((set, get) => ({
  books: [],
  loading: false,
  error: null,
  selectedIds: new Set(),
  progress: {},
  recentBookId: null,

  loadBooks: async () => {
    set({ loading: true, error: null });
    try {
      const [books, progress] = await Promise.all([
        libraryApi.list(),
        progressApi.list().catch(() => [] as ProgressSummary[]),
      ]);
      const picked = pickRecent(books, progress);
      set({
        books,
        loading: false,
        progress: picked.map,
        recentBookId: picked.recentBookId,
      });
    } catch (error) {
      set({ error: String(error), loading: false });
    }
  },

  importBook: async (path) => {
    try {
      const book = await libraryApi.import(path);
      set({ books: [...get().books, book] });
      return book;
    } catch (error) {
      set({ error: String(error) });
      throw error;
    }
  },

  removeBook: async (bookId) => {
    try {
      await libraryApi.remove(bookId);
      const selectedIds = new Set(get().selectedIds);
      selectedIds.delete(bookId);
      const progress = { ...get().progress };
      delete progress[bookId];
      set({
        books: get().books.filter((book) => book.id !== bookId),
        selectedIds,
        progress,
        recentBookId: get().recentBookId === bookId ? null : get().recentBookId,
      });
    } catch (error) {
      set({ error: String(error) });
    }
  },

  batchRemoveBooks: async (bookIds) => {
    try {
      await libraryApi.batchRemove(bookIds);
      const removed = new Set(bookIds);
      const progress = { ...get().progress };
      for (const id of bookIds) delete progress[id];
      const recent = get().recentBookId;
      set({
        books: get().books.filter((book) => !removed.has(book.id)),
        selectedIds: new Set(),
        progress,
        recentBookId: recent && removed.has(recent) ? null : recent,
      });
    } catch (error) {
      await get().loadBooks();
      set({ error: String(error), selectedIds: new Set() });
    }
  },

  toggleSelect: (bookId) => {
    const selectedIds = new Set(get().selectedIds);
    if (selectedIds.has(bookId)) {
      selectedIds.delete(bookId);
    } else {
      selectedIds.add(bookId);
    }
    set({ selectedIds });
  },

  selectAll: () => {
    set({ selectedIds: new Set(get().books.map((book) => book.id)) });
  },

  clearSelection: () => {
    set({ selectedIds: new Set() });
  },

  clearError: () => {
    set({ error: null });
  },
}));
