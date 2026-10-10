import assert from "node:assert/strict";
import { test } from "node:test";
import { filterAndSort, matchesQuery, statusLabel, statusOf } from "./library-filter.ts";
import type { BookInfo, ReadingStatus } from "../types/index.ts";

function book(id: string, title: string, author: string, addedAt: number): BookInfo {
  return {
    id,
    title,
    author,
    cover: null,
    file_path: "books/" + id + ".epub",
    added_at: addedAt,
    file_size: 100,
    format: "epub",
  };
}

const books: BookInfo[] = [
  book("a", "Alpha", "Ann", 100),
  book("b", "beta", "Bob", 300),
  book("c", "Gamma", "ann", 200),
];

test("query matches title and author case-insensitively", () => {
  assert.equal(matchesQuery(books[0], "ALPHA"), true);
  assert.equal(matchesQuery(books[0], "ann"), true);
  assert.equal(matchesQuery(books[0], "  Ann "), true);
  assert.equal(matchesQuery(books[0], "zzz"), false);
});

test("status falls back to progress when no reading data exists", () => {
  assert.equal(statusOf("x", {}, false), "unread");
  assert.equal(statusOf("x", {}, true), "reading");
  assert.equal(statusOf("x", { x: { status: "finished" } }, true), "finished");
});

test("status labels are human readable", () => {
  const labels: ReadingStatus[] = ["unread", "reading", "finished"];
  assert.deepEqual(labels.map(statusLabel), ["未读", "在读", "已读"]);
});

test("recent sort puts unread books last and is stable", () => {
  const progress = { b: { last_read: 50 }, c: { last_read: 900 } };
  const sorted = filterAndSort(books, progress, {}, "", "all", "recent");
  assert.deepEqual(sorted.map((item) => item.book.id), ["c", "b", "a"]);
});

test("added sort uses added time descending", () => {
  const sorted = filterAndSort(books, {}, {}, "", "all", "added");
  assert.deepEqual(sorted.map((item) => item.book.id), ["b", "c", "a"]);
});

test("title sort is alphabetical and stable for equal titles", () => {
  const sorted = filterAndSort(books, {}, {}, "", "all", "title");
  assert.deepEqual(sorted.map((item) => item.book.title), ["Alpha", "beta", "Gamma"]);
});

test("status filter and query combine", () => {
  const summaries = { b: { status: "finished" as ReadingStatus } };
  const progress = { c: { last_read: 10 } };
  const sorted = filterAndSort(books, progress, summaries, "a", "reading", "recent");
  assert.deepEqual(sorted.map((item) => item.book.id), ["c"]);
});
