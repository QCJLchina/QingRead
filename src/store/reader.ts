import { create } from "zustand";
import { progressApi, readerApi } from "../api";
import type { ChapterData, ReaderMode, ReadingProgress, TextAnchor, TocEntry } from "../types";

/** 打开 / 切换章节时希望恢复到什么位置。排版引擎消费一次后清空。 */
export interface PendingPosition {
  anchor: TextAnchor | null;
  /** 旧数据或按页恢复时使用 */
  page: number | null;
  ratio: number | null;
  /** 从下一章往回翻时，停在该章末尾 */
  atEnd?: boolean;
}

export interface OpenChapterOptions {
  anchor?: TextAnchor | null;
  page?: number | null;
  ratio?: number | null;
  atEnd?: boolean;
}

export interface SaveProgressInput {
  chapterIndex: number;
  pageInChapter: number;
  anchor: TextAnchor | null;
  mode: ReaderMode;
}

interface ReaderState {
  bookId: string | null;
  chapterCount: number;
  currentChapter: ChapterData | null;
  toc: TocEntry[];
  progress: ReadingProgress | null;
  /** offsets[i] = 第 i 章之前的累计页数，用于 total_pages_read */
  chapterOffsets: number[];
  loading: boolean;
  error: string | null;
  /** 正在加载的目标章节，用来在保留旧内容时提示「正在加载」 */
  loadingChapter: number | null;
  pendingPosition: PendingPosition | null;

  openBook: (bookId: string) => Promise<void>;
  loadChapter: (chapterIndex: number, options?: OpenChapterOptions) => Promise<void>;
  preloadChapter: (chapterIndex: number) => void;
  consumePendingPosition: () => PendingPosition | null;
  saveProgress: (input: SaveProgressInput) => void;
  retry: () => Promise<void>;
  clearReader: () => void;
}

/**
 * 请求序号：快速切书或连点章节时，旧请求的返回必须被丢弃，
 * 否则会出现「点第 3 章，屏幕上是第 2 章」这种串内容。
 */
let bookToken = 0;
let chapterToken = 0;

/**
 * 每本书一条串行写入队列。
 * 保存进度是「最后一次为准」，但并发 IPC 的完成顺序不保证，
 * 排队可以避免较早的请求把较新的位置覆盖掉。
 */
const saveQueues = new Map<string, Promise<void>>();

function enqueueSave(bookId: string, task: () => Promise<void>): void {
  const previous = saveQueues.get(bookId) ?? Promise.resolve();
  const next = previous
    .catch(() => undefined)
    .then(task)
    .catch((error) => {
      console.error("保存阅读进度失败:", error);
    });
  saveQueues.set(bookId, next);
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
  loadingChapter: null,
  pendingPosition: null,

  openBook: async (bookId) => {
    const token = ++bookToken;
    chapterToken += 1;
    set({
      loading: true,
      error: null,
      bookId,
      chapterCount: 0,
      toc: [],
      progress: null,
      chapterOffsets: [0],
      pendingPosition: null,
      loadingChapter: null,
    });

    try {
      const [chapterCount, progress, toc, offsets] = await Promise.all([
        readerApi.open(bookId),
        progressApi.load(bookId).catch(() => null),
        readerApi.toc(bookId).catch(() => [] as TocEntry[]),
        readerApi.chapterOffsets(bookId).catch(() => null),
      ]);
      if (token !== bookToken) return;

      set({
        chapterCount,
        progress,
        toc,
        chapterOffsets:
          offsets ?? Array.from({ length: chapterCount + 1 }, (_, index) => index),
      });

      const startChapter = progress?.chapter_index ?? 0;
      await get().loadChapter(startChapter, {
        anchor: progress?.anchor ?? null,
        page: progress?.page_in_chapter ?? null,
      });
      if (token !== bookToken) return;
      set({ loading: false });
    } catch (error) {
      if (token !== bookToken) return;
      set({ error: String(error), loading: false });
    }
  },

  loadChapter: async (chapterIndex, options) => {
    const { bookId } = get();
    if (!bookId) return;

    const token = ++chapterToken;
    // 注意：不清空 currentChapter。缓慢章节解析期间保留上一屏内容，
    // 用户看到的是「还在加载」，而不是白屏。
    set({
      loading: true,
      loadingChapter: chapterIndex,
      error: null,
      pendingPosition: {
        anchor: options?.anchor ?? null,
        page: options?.page ?? null,
        ratio: options?.ratio ?? null,
        atEnd: options?.atEnd ?? false,
      },
    });

    try {
      const chapter = await readerApi.chapter(bookId, chapterIndex);
      if (token !== chapterToken) return;
      set({
        currentChapter: chapter,
        loading: false,
        loadingChapter: null,
        error: null,
      });
    } catch (error) {
      if (token !== chapterToken) return;
      // 保留 loadingChapter：重试时要知道失败的是哪一章
      set({
        loading: false,
        error: String(error),
        pendingPosition: null,
      });
    }
  },

  preloadChapter: (chapterIndex) => {
    const { bookId, chapterCount } = get();
    if (!bookId || chapterIndex < 0 || chapterIndex >= chapterCount) return;
    readerApi.chapter(bookId, chapterIndex).catch(() => undefined);
  },

  consumePendingPosition: () => {
    const pending = get().pendingPosition;
    if (pending) set({ pendingPosition: null });
    return pending;
  },

  saveProgress: (input) => {
    const { bookId, chapterOffsets } = get();
    if (!bookId) return;
    const base = chapterOffsets[input.chapterIndex] ?? input.chapterIndex;
    const payload = {
      bookId,
      chapterIndex: input.chapterIndex,
      pageInChapter: Math.max(0, Math.floor(input.pageInChapter)),
      totalPagesRead: Math.max(0, base + Math.floor(input.pageInChapter) + 1),
      anchor: input.anchor,
      mode: input.mode,
    };

    set({
      progress: {
        book_id: bookId,
        chapter_index: payload.chapterIndex,
        page_in_chapter: payload.pageInChapter,
        total_pages_read: payload.totalPagesRead,
        last_read: Math.floor(Date.now() / 1000),
        anchor: payload.anchor,
        mode: payload.mode,
      },
    });

    enqueueSave(bookId, () => progressApi.save(payload));
  },

  retry: async () => {
    const { currentChapter } = get();
    await get().loadChapter(currentChapter?.index ?? 0);
  },

  clearReader: () => {
    bookToken += 1;
    chapterToken += 1;
    set({
      bookId: null,
      chapterCount: 0,
      currentChapter: null,
      toc: [],
      progress: null,
      chapterOffsets: [0],
      loading: false,
      error: null,
      loadingChapter: null,
      pendingPosition: null,
    });
  },
}));
