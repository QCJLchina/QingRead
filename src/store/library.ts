import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type { BookInfo } from "../types";

interface LibraryState {
  books: BookInfo[];
  loading: boolean;
  error: string | null;
  selectedIds: Set<string>;

  loadBooks: () => Promise<void>;
  importBook: (path: string) => Promise<BookInfo | null>;
  removeBook: (bookId: string) => Promise<void>;
  batchRemoveBooks: (bookIds: string[]) => Promise<void>;
  toggleSelect: (bookId: string) => void;
  selectAll: () => void;
  clearSelection: () => void;
  clearError: () => void;
}

export const useLibraryStore = create<LibraryState>((set, get) => ({
  books: [],
  loading: false,
  error: null,
  selectedIds: new Set(),

  loadBooks: async () => {
    set({ loading: true, error: null });
    try {
      const books = await invoke<BookInfo[]>("list_books");
      set({ books, loading: false });
    } catch (e) {
      set({ error: String(e), loading: false });
    }
  },

  importBook: async (path: string) => {
    try {
      const book = await invoke<BookInfo>("import_book", { path });
      set({ books: [...get().books, book] });
      return book;
    } catch (e) {
      set({ error: String(e) });
      throw e;
    }
  },

  removeBook: async (bookId: string) => {
    try {
      await invoke("remove_book", { bookId });
      const { selectedIds } = get();
      const newSelected = new Set(selectedIds);
      newSelected.delete(bookId);
      set({
        books: get().books.filter((b) => b.id !== bookId),
        selectedIds: newSelected,
      });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  batchRemoveBooks: async (bookIds: string[]) => {
    try {
      await invoke("batch_remove_books", { bookIds });
      const idSet = new Set(bookIds);
      set({
        books: get().books.filter((b) => !idSet.has(b.id)),
        selectedIds: new Set(),
      });
    } catch (e) {
      await get().loadBooks();
      set({ error: String(e), selectedIds: new Set() });
    }
  },

  toggleSelect: (bookId: string) => {
    const { selectedIds } = get();
    const newSelected = new Set(selectedIds);
    if (newSelected.has(bookId)) {
      newSelected.delete(bookId);
    } else {
      newSelected.add(bookId);
    }
    set({ selectedIds: newSelected });
  },

  selectAll: () => {
    const { books } = get();
    set({ selectedIds: new Set(books.map((b) => b.id)) });
  },

  clearSelection: () => {
    set({ selectedIds: new Set() });
  },

  clearError: () => {
    set({ error: null });
  },
}));
