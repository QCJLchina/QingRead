import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type { ChapterData, ReadingProgress, TocEntry } from "../types";

interface ReaderState {
  bookId: string | null;
  chapterCount: number;
  currentChapter: ChapterData | null;
  toc: TocEntry[];
  progress: ReadingProgress | null;
  chapterOffsets: number[]; // offsets[i] = 第一章之前的累计页数（offsets[0] = 0, offsets[1] = 第1章页数, ...）
  loading: boolean;
  error: string | null;

  openBook: (bookId: string) => Promise<void>;
  loadChapter: (chapterIndex: number) => Promise<void>;
  preloadChapter: (chapterIndex: number) => Promise<void>;
  loadToc: () => Promise<void>;
  loadProgress: () => Promise<void>;
  loadChapterOffsets: () => Promise<void>;
  saveProgress: (chapterIndex: number, pageInChapter: number, totalPagesRead: number) => Promise<void>;
  clearReader: () => void;
}

export const useReaderStore = create<ReaderState>((set, get) => ({
  bookId: null,
  chapterCount: 0,
  currentChapter: null,
  toc: [],
  progress: null,
  chapterOffsets: [0],
  loading: false,
  error: null,

  openBook: async (bookId: string) => {
    set({ loading: true, error: null, bookId, chapterOffsets: [0] });
    try {
      // 并行执行 4 个互不依赖的 IPC 调用，节省 3 轮网络往返时间
      const [chapterCount, progress, toc, offsets] = await Promise.all([
        invoke<number>("open_reader", { bookId }),
        invoke<ReadingProgress | null>("load_progress", { bookId }).catch(() => null),
        invoke<TocEntry[]>("get_toc", { bookId }).catch(() => [] as TocEntry[]),
        invoke<number[]>("get_chapter_offsets", { bookId }).catch(() => null),
      ]);

      set({
        chapterCount,
        progress,
        toc,
        chapterOffsets: offsets ?? Array.from({ length: chapterCount + 1 }, (_, i) => i),
      });

      // Load first chapter or resume from progress
      const startChapter = progress?.chapter_index ?? 0;
      await get().loadChapter(startChapter);

      set({ loading: false });
    } catch (e) {
      set({ error: String(e), loading: false });
    }
  },

  loadChapter: async (chapterIndex: number) => {
    const { bookId } = get();
    if (!bookId) return;

    set({ loading: true, error: null });
    try {
      const chapter = await invoke<ChapterData>("load_chapter", {
        bookId,
        chapterIndex,
      });
      set({ currentChapter: chapter, loading: false });
    } catch (e) {
      set({ error: String(e), loading: false });
    }
  },

  /// 后台预加载章节（不阻塞 UI）
  preloadChapter: async (chapterIndex: number) => {
    const { bookId } = get();
    if (!bookId) return;
    try {
      await invoke<ChapterData>("load_chapter", {
        bookId,
        chapterIndex,
      });
    } catch {
      // 预加载失败忽略
    }
  },

  loadToc: async () => {
    const { bookId } = get();
    if (!bookId) return;
    try {
      const toc = await invoke<TocEntry[]>("get_toc", { bookId });
      set({ toc });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  loadProgress: async () => {
    const { bookId } = get();
    if (!bookId) return;
    try {
      const progress = await invoke<ReadingProgress | null>("load_progress", { bookId });
      set({ progress });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  loadChapterOffsets: async () => {
    const { bookId } = get();
    if (!bookId) return;
    try {
      const offsets = await invoke<number[]>("get_chapter_offsets", { bookId });
      set({ chapterOffsets: offsets });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  saveProgress: async (chapterIndex: number, pageInChapter: number, totalPagesRead: number) => {
    const { bookId } = get();
    if (!bookId) return;
    try {
      await invoke("save_progress", {
        bookId,
        chapterIndex,
        pageInChapter,
        totalPagesRead,
      });
      set({
        progress: {
          book_id: bookId,
          chapter_index: chapterIndex,
          page_in_chapter: pageInChapter,
          total_pages_read: totalPagesRead,
          last_read: Date.now(),
        },
      });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  clearReader: () => {
    set({
      bookId: null,
      chapterCount: 0,
      currentChapter: null,
      toc: [],
      progress: null,
      chapterOffsets: [0],
      loading: false,
      error: null,
    });
  },
}));
