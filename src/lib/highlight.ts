/**
 * 把搜索命中的那一段文字高亮出来。
 *
 * 用 CSS Custom Highlight API（WebView2 支持）而不是往正文里插 <mark>：
 * 正文是 dangerouslySetInnerHTML 渲染的，动 DOM 会和 React 的更新打架。
 * 不支持时静默跳过 —— 位置跳转本身仍然有效。
 */
import { findTextPosition, nodeToPath } from "./anchor";
import { textModel } from "./text-model";
import type { NodeLike } from "./anchor";
import type { TextAnchor } from "../types";

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

/**
 * 按正文纯文本里的字符位置定位。
 *
 * 搜索结果的 position 是 stripHtml 之后文本的偏移，和 DOM 里的原始文本可能差在
 * 实体与空白折叠上，所以先用同一条归一化规则映射回原始下标，再转成锚点，
 * 交给排版引擎滚到对应页。这样重复句子也能跳到正确的位置。
 */
export function anchorAtPlainText(root: HTMLElement, position: number, length: number): TextAnchor | null {
  const model = textModel(root);
  const point = model.points[position];
  if (!point) return null;
  const owner = point.node;
  const offset = point.offset;
  const path = nodeToPath(root as unknown as NodeLike, owner as unknown as NodeLike);
  if (!path) return null;


  return {
    path,
    offset,
    snippet: owner.data.slice(offset, Math.min(owner.data.length, offset + 48)),
    ratio: 0,
  };
}

/** 在指定纯文本位置插入高亮。 */
export function applyHighlightAtPosition(
  root: HTMLElement,
  position: number,
  length: number,
): boolean {
  const registry = highlightRegistry();
  const HighlightCtor = (globalThis as { Highlight?: new (...ranges: Range[]) => unknown }).Highlight;
  if (!registry || typeof HighlightCtor !== "function") return false;

  const anchor = anchorAtPlainText(root, position, length);
  if (!anchor) return false;

  const node = pathToText(root, anchor.path);
  if (!node) return false;

  const range = document.createRange();
  range.setStart(node, Math.min(anchor.offset, node.data.length));
  const end = textModel(root).points[position + Math.max(1, length) - 1];
  if (!end) return false;
  range.setEnd(end.node, Math.min(end.node.data.length, end.offset + 1));
  registry.set(HIGHLIGHT_NAME, new HighlightCtor(range));

  return true;
}

function pathToText(root: HTMLElement, path: number[]): Text | null {
  let current: Node = root;
  for (const index of path) {
    const next: Node | undefined = current.childNodes[index];
    if (!next) return null;
    current = next;
  }
  return current.nodeType === 3 ? (current as Text) : null;
}
