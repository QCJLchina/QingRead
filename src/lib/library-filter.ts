import type { BookInfo, LibraryFilter, LibrarySort, ReadingStatus } from "../types";

export interface ProgressLike {
  last_read: number;
}

export interface LibrarySortItem {
  book: BookInfo;
  lastRead: number;
  status: ReadingStatus;
}

export function normalizeQuery(value: string): string {
  return value.trim().toLowerCase();
}

export function matchesQuery(book: BookInfo, query: string): boolean {
  const needle = normalizeQuery(query);
  if (!needle) return true;
  return (
    book.title.toLowerCase().includes(needle) ||
    (book.author || "").toLowerCase().includes(needle)
  );
}

export function statusOf(
  bookId: string,
  summaries: Record<string, { status: ReadingStatus }>,
  hasProgress: boolean,
): ReadingStatus {
  const explicit = summaries[bookId]?.status;
  if (explicit) return explicit;
  // 阅读资料文件不存在时按旧进度推导：有进度即为在读。
  return hasProgress ? "reading" : "unread";
}

export function statusLabel(status: ReadingStatus): string {
  switch (status) {
    case "reading":
      return "在读";
    case "finished":
      return "已读";
    default:
      return "未读";
  }
}

function tieBreak(a: LibrarySortItem, b: LibrarySortItem): number {
  const byAdded = b.book.added_at - a.book.added_at;
  if (byAdded !== 0) return byAdded;
  return a.book.id.localeCompare(b.book.id);
}

/**
 * 排序规则：
 * - recent：最近阅读优先，未读（last_read = 0）排在其后；
 * - added：按添加时间倒序；
 * - title：按书名排序。
 * 同值时以添加时间倒序、书籍 ID 兜底，保证排序稳定。
 */
export function sortBooks(items: LibrarySortItem[], sort: LibrarySort): LibrarySortItem[] {
  const sorted = [...items];
  sorted.sort((a, b) => {
    if (sort === "title") {
      const byTitle = a.book.title.localeCompare(b.book.title, "zh-Hans-CN");
      if (byTitle !== 0) return byTitle;
      return tieBreak(a, b);
    }
    if (sort === "recent") {
      const byRecent = b.lastRead - a.lastRead;
      if (byRecent !== 0) return byRecent;
      return tieBreak(a, b);
    }
    const byAdded = b.book.added_at - a.book.added_at;
    if (byAdded !== 0) return byAdded;
    return tieBreak(a, b);
  });
  return sorted;
}

export function filterByStatus(
  items: LibrarySortItem[],
  filter: LibraryFilter,
): LibrarySortItem[] {
  if (filter === "all") return items;
  return items.filter((item) => item.status === filter);
}

export function filterAndSort(
  books: BookInfo[],
  progress: Record<string, ProgressLike>,
  summaries: Record<string, { status: ReadingStatus }>,
  query: string,
  filter: LibraryFilter,
  sort: LibrarySort,
): LibrarySortItem[] {
  const items: LibrarySortItem[] = books
    .filter((book) => matchesQuery(book, query))
    .map((book) => {
      const entry = progress[book.id];
      return {
        book,
        lastRead: entry?.last_read ?? 0,
        status: statusOf(book.id, summaries, Boolean(entry)),
      };
    });
  return sortBooks(filterByStatus(items, filter), sort);
}
