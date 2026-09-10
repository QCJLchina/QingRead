/**
 * 把搜索命中的那一段文字高亮出来。
 *
 * 用 CSS Custom Highlight API（WebView2 支持）而不是往正文里插 <mark>：
 * 正文是 dangerouslySetInnerHTML 渲染的，动 DOM 会和 React 的更新打架。
 * 不支持时静默跳过 —— 位置跳转本身仍然有效。
 */
import { findTextPosition } from "./anchor";

const HIGHLIGHT_NAME = "reader-search-match";

function highlightRegistry(): { set: (name: string, value: unknown) => void; delete: (name: string) => void } | null {
  const css = (globalThis as { CSS?: { highlights?: unknown } }).CSS;
  const highlights = css?.highlights as
    | { set: (name: string, value: unknown) => void; delete: (name: string) => void }
    | undefined;
  return highlights ?? null;
}

export function clearMatchHighlight(): void {
  highlightRegistry()?.delete(HIGHLIGHT_NAME);
}

export function applyMatchHighlight(root: HTMLElement, snippet: string): boolean {
  const registry = highlightRegistry();
  const HighlightCtor = (globalThis as { Highlight?: new (...ranges: Range[]) => unknown }).Highlight;
  if (!registry || typeof HighlightCtor !== "function") return false;

  const position = findTextPosition(root, snippet);
  if (!position) return false;

  const range = document.createRange();
  range.setStart(position.node, Math.min(position.offset, position.node.data.length));
  range.setEnd(
    position.node,
    Math.min(position.offset + snippet.length, position.node.data.length),
  );
  registry.set(HIGHLIGHT_NAME, new HighlightCtor(range));
  return true;
}
