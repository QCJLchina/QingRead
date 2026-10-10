import type { Bookmark, TextAnchor } from "../types/index.ts";
export function compareAnchors(a: TextAnchor, b: TextAnchor): number {
  for (let i = 0; i < Math.min(a.path.length, b.path.length); i += 1) {
    if (a.path[i] !== b.path[i]) return a.path[i] - b.path[i];
  }
  return a.path.length - b.path.length || a.offset - b.offset;
}
export function compareBookmarks(a: Bookmark, b: Bookmark): number {
  return a.chapter_index - b.chapter_index || compareAnchors(a.anchor, b.anchor) || a.created_at - b.created_at || a.id.localeCompare(b.id);
}
