import assert from "node:assert/strict";
import test from "node:test";
import { stripHtml } from "./search.ts";
import { useLibraryStore } from "../store/library.ts";

test("stripHtml removes tags and normalizes whitespace", () => {
  assert.equal(
    stripHtml("<h1>Title</h1>\n<p>First   paragraph</p>"),
    "Title First paragraph"
  );
});

test("stripHtml preserves readable text around empty elements", () => {
  assert.equal(stripHtml("Before<br><img src=\"cover.jpg\">After"), "Before After");
});

test("library selection actions keep selected IDs consistent", () => {
  const store = useLibraryStore;
  store.setState({
    books: [
      { id: "book-1", title: "One", author: "", cover: null, file_path: "", added_at: 0, file_size: 0, format: "epub" },
      { id: "book-2", title: "Two", author: "", cover: null, file_path: "", added_at: 0, file_size: 0, format: "epub" },
    ],
    selectedIds: new Set(),
  });

  store.getState().toggleSelect("book-1");
  assert.deepEqual([...store.getState().selectedIds], ["book-1"]);
  store.getState().selectAll();
  assert.deepEqual([...store.getState().selectedIds].sort(), ["book-1", "book-2"]);
  store.getState().clearSelection();
  assert.equal(store.getState().selectedIds.size, 0);
});
