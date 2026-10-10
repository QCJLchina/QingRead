import { create } from "zustand";
import { readingApi } from "../api";
import type {
  Bookmark,
  ReadingData,
  ReadingStatus,
  ReadingSummary,
  TextAnchor,
} from "../types";

const writes = new Map<string, Promise<unknown>>();
function write<T>(bookId: string, task: () => Promise<T>): Promise<T> {
  const next = (writes.get(bookId) ?? Promise.resolve()).catch(() => undefined).then(task);
  writes.set(bookId, next);
  void next.catch(() => undefined);
  return next;
}
export async function flushReadingWrites(): Promise<void> {
  await Promise.all([...writes.values()]);
}

interface ReadingState {
  /** book_id -> 完整阅读资料（当前打开的书会懒加载） */
  data: Record<string, ReadingData>;
  /** 书架筛选用的汇总 */
  summaries: Record<string, ReadingSummary>;
  loadedBookId: string | null;
  saving: boolean;
  error: string | null;

  loadSummaries: () => Promise<void>;
  loadBook: (bookId: string) => Promise<ReadingData>;
  ensureStatus: (bookId: string, status: ReadingStatus) => Promise<void>;
  setStatus: (bookId: string, status: ReadingStatus) => Promise<void>;
  addBookmark: (payload: {
    bookId: string;
    chapterIndex: number;
    anchor: TextAnchor | null;
    snippet: string;
    note?: string;
  }) => Promise<{ created: boolean; bookmark: Bookmark | null }>;
  updateBookmark: (bookId: string, bookmarkId: string, note: string) => Promise<void>;
  removeBookmark: (bookId: string, bookmarkId: string) => Promise<void>;
  appendHistory: (payload: {
    bookId: string;
    kind: "open" | "leave" | "jump";
    chapterIndex: number;
    anchor: TextAnchor | null;
    snippet: string;
  }) => Promise<void>;
  clearHistory: (bookId: string) => Promise<void>;
  dropBook: (bookId: string) => void;
  clearError: () => void;
}

function summaryOf(data: ReadingData): ReadingSummary {
  return {
    book_id: data.book_id,
    status: data.status,
    bookmark_count: data.bookmarks.length,
    history_count: data.history.length,
    updated_at: data.updated_at,
  };
}

export const useReadingStore = create<ReadingState>((set, get) => ({
  data: {},
  summaries: {},
  loadedBookId: null,
  saving: false,
  error: null,

  loadSummaries: async () => {
    try {
      const list = await readingApi.summaries();
      const map: Record<string, ReadingSummary> = {};
      for (const item of list) map[item.book_id] = item;
      set({ summaries: map });
    } catch (error) {
      console.error("加载阅读状态失败:", error);
    }
  },

  loadBook: async (bookId) => {
    const existing = get().data[bookId];
    if (existing) return existing;
    try {
      const data = await readingApi.get(bookId);
      set((state) => ({
        data: { ...state.data, [bookId]: data },
        summaries: { ...state.summaries, [bookId]: summaryOf(data) },
        loadedBookId: bookId,
      }));
      return data;
    } catch (error) {
      console.error("加载阅读资料失败:", error);
      set({ error: String(error) });
      throw error;
    }
  },

  /** 首次显示正文后把未读推进为在读；已读不会被降级。 */
  ensureStatus: async (bookId, status) => {
    await get().loadBook(bookId);
    const data = await write(bookId, () => readingApi.markStarted(bookId));
    set((state) => ({
      data: { ...state.data, [bookId]: data },
      summaries: { ...state.summaries, [bookId]: summaryOf(data) },
    }));
  },

  setStatus: async (bookId, status) => {
    try {
      const data = await write(bookId, () => readingApi.setStatus(bookId, status));
      set((state) => ({
        data: { ...state.data, [bookId]: data },
        summaries: { ...state.summaries, [bookId]: summaryOf(data) },
      }));
    } catch (error) {
      set({ error: String(error) });
    }
  },

  addBookmark: async (payload) => {
    set({ saving: true, error: null });
    try {
      const result = await write(payload.bookId, () => readingApi.addBookmark({
        bookId: payload.bookId,
        chapterIndex: payload.chapterIndex,
        anchor: payload.anchor,
        snippet: payload.snippet,
        note: payload.note ?? null,
      }));
      const data = result.data;
      set((state) => ({
        data: { ...state.data, [payload.bookId]: data },
        summaries: { ...state.summaries, [payload.bookId]: summaryOf(data) },
        saving: false,
      }));
      const bookmark =
        data.bookmarks.find(
          (item) =>
            item.chapter_index === payload.chapterIndex &&
            item.anchor.offset === payload.anchor?.offset &&
            JSON.stringify(item.anchor.path) === JSON.stringify(payload.anchor?.path ?? []),
        ) ?? null;
      return { created: result.created, bookmark };
    } catch (error) {
      set({ saving: false, error: String(error) });
      throw error;
    }
  },

  updateBookmark: async (bookId, bookmarkId, note) => {
    set({ saving: true, error: null });
    try {
      const data = await write(bookId, () => readingApi.updateBookmark(bookId, bookmarkId, note || null));
      set((state) => ({
        data: { ...state.data, [bookId]: data },
        summaries: { ...state.summaries, [bookId]: summaryOf(data) },
        saving: false,
      }));
    } catch (error) {
      set({ saving: false, error: String(error) });
      throw error;
    }
  },

  removeBookmark: async (bookId, bookmarkId) => {
    set({ saving: true, error: null });
    try {
      const data = await write(bookId, () => readingApi.deleteBookmark(bookId, bookmarkId));
      set((state) => ({
        data: { ...state.data, [bookId]: data },
        summaries: { ...state.summaries, [bookId]: summaryOf(data) },
        saving: false,
      }));
    } catch (error) {
      set({ saving: false, error: String(error) });
    }
  },

  appendHistory: async (payload) => {
    try {
      const data = await write(payload.bookId, () => readingApi.appendHistory(payload));
      set((state) => ({
        data: { ...state.data, [payload.bookId]: data },
        summaries: { ...state.summaries, [payload.bookId]: summaryOf(data) },
      }));
    } catch (error) {
      console.error("保存阅读历史失败:", error);
      set({ error: String(error) });
      throw error;
    }
  },

  clearHistory: async (bookId) => {
    try {
      const data = await write(bookId, () => readingApi.clearHistory(bookId));
      set((state) => ({
        data: { ...state.data, [bookId]: data },
        summaries: { ...state.summaries, [bookId]: summaryOf(data) },
      }));
    } catch (error) {
      set({ error: String(error) });
    }
  },

  dropBook: (bookId) => {
    set((state) => {
      const data = { ...state.data };
      delete data[bookId];
      const summaries = { ...state.summaries };
      delete summaries[bookId];
      return { data, summaries };
    });
  },

  clearError: () => set({ error: null }),
}));

export type { Bookmark };
