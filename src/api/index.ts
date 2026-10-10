/**
 * 类型化的桌面接口层。
 *
 * 页面组件只跟这里打交道，不直接 invoke —— 命令名、参数名、返回类型都在一处，
 * 重构时不会出现「某个页面还在用旧参数名」的静默失败。
 */
import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  Bookmark,
  BookInfo,
  ChapterData,
  HistoryEntry,
  ReaderMode,
  ReadingData,
  ReadingStatus,
  ReadingSummary,
  ReadingProgress,
  SyncConfigData,
  SyncDecision,
  SyncPreview,
  SyncSummary,
  TextAnchor,
  TocEntry,
  WindowSettings,
} from "../types";

export const libraryApi = {
  list: () => invoke<BookInfo[]>("list_books"),
  import: (path: string) => invoke<BookInfo>("import_book", { path }),
  remove: (bookId: string) => invoke<void>("remove_book", { bookId }),
  batchRemove: (bookIds: string[]) => invoke<void>("batch_remove_books", { bookIds }),
  cover: (bookId: string) => invoke<string | null>("get_cover", { bookId }),
};

export const readerApi = {
  open: (bookId: string) => invoke<number>("open_reader", { bookId }),
  chapter: (bookId: string, chapterIndex: number) =>
    invoke<ChapterData>("load_chapter", { bookId, chapterIndex }),
  toc: (bookId: string) => invoke<TocEntry[]>("get_toc", { bookId }),
  chapterOffsets: (bookId: string) => invoke<number[]>("get_chapter_offsets", { bookId }),
};

export interface ProgressSummary {
  book_id: string;
  chapter_index: number;
  total_pages_read: number;
  last_read: number;
}

export const progressApi = {
  load: (bookId: string) => invoke<ReadingProgress | null>("load_progress", { bookId }),
  list: () => invoke<ProgressSummary[]>("list_progress"),
  save: (payload: {
    bookId: string;
    chapterIndex: number;
    pageInChapter: number;
    totalPagesRead: number;
    anchor: TextAnchor | null;
    mode: ReaderMode;
  }) =>
    invoke<void>("save_progress", {
      bookId: payload.bookId,
      chapterIndex: payload.chapterIndex,
      pageInChapter: payload.pageInChapter,
      totalPagesRead: payload.totalPagesRead,
      anchor: payload.anchor,
      mode: payload.mode,
    }),
};

export const readingApi = {
  get: (bookId: string) => invoke<ReadingData>("get_reading_data", { bookId }),
  summaries: () => invoke<ReadingSummary[]>("list_reading_summaries"),
  setStatus: (bookId: string, status: ReadingStatus) =>
    invoke<ReadingData>("set_reading_status", { bookId, status }),
  markStarted: (bookId: string) => invoke<ReadingData>("mark_reading_started", { bookId }),
  addBookmark: (payload: {
    bookId: string;
    chapterIndex: number;
    anchor: TextAnchor | null;
    snippet: string;
    note?: string | null;
  }) =>
    invoke<{ data: ReadingData; created: boolean }>("add_bookmark", {
      bookId: payload.bookId,
      chapterIndex: payload.chapterIndex,
      anchor: payload.anchor,
      snippet: payload.snippet,
      note: payload.note ?? null,
    }),
  updateBookmark: (bookId: string, bookmarkId: string, note: string | null) =>
    invoke<ReadingData>("update_bookmark", { bookId, bookmarkId, note }),
  deleteBookmark: (bookId: string, bookmarkId: string) =>
    invoke<ReadingData>("delete_bookmark", { bookId, bookmarkId }),
  appendHistory: (payload: {
    bookId: string;
    kind: "open" | "leave" | "jump";
    chapterIndex: number;
    anchor: TextAnchor | null;
    snippet: string;
  }) =>
    invoke<ReadingData>("append_history", {
      bookId: payload.bookId,
      kind: payload.kind,
      chapterIndex: payload.chapterIndex,
      anchor: payload.anchor,
      snippet: payload.snippet,
    }),
  clearHistory: (bookId: string) => invoke<ReadingData>("clear_history", { bookId }),
};


export type { Bookmark, HistoryEntry, ReadingData, ReadingSummary };

export const settingsApi = {
  get: () => invoke<AppSettings>("get_settings"),
  save: (settings: AppSettings) => invoke<void>("save_settings", { settings }),
  setDataDir: (path: string) => invoke<void>("set_data_dir", { path }),
  restart: () => invoke<void>("restart_app"),
};

export const syncApi = {
  getConfig: () => invoke<SyncConfigData>("get_sync_config"),
  setConfig: (payload: {
    serverUrl: string;
    username: string;
    password: string;
    remoteDir: string;
  }) => invoke<void>("set_sync_config", payload),
  clearConfig: () => invoke<void>("clear_sync_config"),
  testConnection: (password: string | null) =>
    invoke<void>("test_sync_connection", { password }),
  preview: () => invoke<SyncPreview>("preview_sync"),
  apply: (payload: {
    decisions: SyncDecision[];
    expectedLocalFingerprint: string;
    expectedRemoteFingerprint: string;
  }) => invoke<SyncSummary>("apply_sync", {
    decisions: payload.decisions,
    expectedLocalFingerprint: payload.expectedLocalFingerprint,
    expectedRemoteFingerprint: payload.expectedRemoteFingerprint,
  }),
};

export const windowApi = {
  getSettings: () => invoke<WindowSettings>("get_window_settings"),
  saveSettings: (settings: WindowSettings) =>
    invoke<void>("save_window_settings", { settings }),
  applyLayout: (payload: {
    width: number;
    height: number;
    layout: string;
    lockRatio: boolean;
  }) =>
    invoke<void>("window_apply_layout", {
      width: payload.width,
      height: payload.height,
      layout: payload.layout,
      lockRatio: payload.lockRatio,
    }),
  hide: () => invoke<void>("window_hide"),
};

export const appApi = {
  version: () => invoke<string>("get_app_version"),
  revealInFolder: (path: string) => invoke<void>("reveal_in_folder", { path }),
};
