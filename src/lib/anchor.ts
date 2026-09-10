/**
 * 稳定阅读位置：文本锚点。
 *
 * 页码会随窗口宽高、字号、行距、字体加载而变化，所以不能拿页码当位置。
 * 这里保存的是「清洗后正文里的第几个文本节点的第几个字符」，并附带一段
 * 附近文本和章节内比例，作为路径失效时的回退。
 *
 * 纯计算部分（路径换算、片段匹配、空白折叠）刻意不碰 DOM，便于单元测试；
 * 依赖浏览器 API 的部分集中在文件后半段。
 */
import type { TextAnchor } from "../types";

/** 最小结构，既匹配真实 DOM 节点，也便于测试里构造假节点 */
export interface NodeLike {
  childNodes: ArrayLike<NodeLike>;
  parentNode?: NodeLike | null;
}

const SNIPPET_RADIUS = 24;
/** 校验偏移是否仍然可信时，向前读取的字符数 */
const MATCH_WINDOW = 6;

export function clampRatio(value: number): number {
  if (!Number.isFinite(value)) return 0;
  return Math.min(1, Math.max(0, value));
}

export function snippetAround(text: string, offset: number, radius = SNIPPET_RADIUS): string {
  const safeOffset = Math.min(Math.max(0, offset), text.length);
  const start = Math.max(0, safeOffset - radius);
  const end = Math.min(text.length, safeOffset + radius + 1);
  return text.slice(start, end);
}

export function findSnippetOffset(text: string, snippet: string): number {
  if (!snippet) return -1;
  return text.indexOf(snippet);
}

/**
 * 记录的偏移是否还指向原来的内容。
 * 找到锚点时会连同前后各 24 个字符一起存下来，所以「从 offset 开始的几个字符」
 * 必然出现在片段里；如果对不上，说明正文变了，改用片段重新定位。
 */
export function snippetLooseMatch(
  text: string,
  offset: number,
  snippet: string,
  window = MATCH_WINDOW,
): boolean {
  if (!snippet) return true;
  const safeOffset = Math.max(0, offset);
  const found = text.indexOf(snippet);
  if (found < 0) return false;
  return safeOffset >= found - window && safeOffset <= found + snippet.length + window;
}

export function collapseWhitespace(value: string): string {
  return value.replace(/\s+/g, " ").trim();
}

function indexOfChild(parent: NodeLike, child: NodeLike): number {
  for (let index = 0; index < parent.childNodes.length; index += 1) {
    if (parent.childNodes[index] === child) return index;
  }
  return -1;
}

/** 从 root 走到 target 的子节点索引链 */
export function nodeToPath(root: NodeLike, target: NodeLike): number[] | null {
  const path: number[] = [];
  let current: NodeLike | undefined | null = target;

  while (current && current !== root) {
    const parent: NodeLike | null = current.parentNode ?? null;
    if (!parent) return null;
    const index = indexOfChild(parent, current);
    if (index < 0) return null;
    path.unshift(index);
    current = parent;
  }

  if (current !== root) return null;
  return path;
}

export function pathToNode(root: NodeLike, path: number[]): NodeLike | null {
  let current: NodeLike = root;
  for (const index of path) {
    const next = current.childNodes[index];
    if (!next) return null;
    current = next;
  }
  return current;
}

export interface TextPosition {
  node: Text;
  offset: number;
}

interface TextPiece {
  node: Text;
  start: number;
}

function isSkippable(node: Node): boolean {
  const parent = node.parentElement;
  if (!parent) return false;
  const tag = parent.tagName;
  return tag === "SCRIPT" || tag === "STYLE" || tag === "NOSCRIPT";
}

/** 按文档顺序收集可读文本节点 */
export function collectTextNodes(root: Node): Text[] {
  const nodes: Text[] = [];
  const doc = root.ownerDocument;
  if (!doc) return nodes;

  const walker = doc.createTreeWalker(root, 4 /* NodeFilter.SHOW_TEXT */);
  let current = walker.nextNode();
  while (current) {
    const text = current as Text;
    if (text.data.length > 0 && !isSkippable(text)) {
      nodes.push(text);
    }
    current = walker.nextNode();
  }
  return nodes;
}

function locateRaw(pieces: TextPiece[], rawIndex: number): TextPosition | null {
  for (let index = pieces.length - 1; index >= 0; index -= 1) {
    const piece = pieces[index];
    if (rawIndex >= piece.start) {
      return { node: piece.node, offset: rawIndex - piece.start };
    }
  }
  return null;
}

/**
 * 在正文里按文本查找位置：先精确匹配，失败后折叠空白再匹配。
 * 搜索结果跳转和锚点回退都用它。
 */
export function findTextPosition(root: Node, needle: string): TextPosition | null {
  if (!needle) return null;

  const pieces: TextPiece[] = [];
  let haystack = "";
  for (const node of collectTextNodes(root)) {
    pieces.push({ node, start: haystack.length });
    haystack += node.data;
  }
  if (!haystack) return null;

  const exact = haystack.indexOf(needle);
  if (exact >= 0) return locateRaw(pieces, exact);

  const wanted = collapseWhitespace(needle);
  if (!wanted) return null;

  // 折叠空白后重新匹配，同时记录每个归一化字符对应的原始下标
  const rawIndexes: number[] = [];
  let normalized = "";
  let lastWasSpace = true;
  for (let index = 0; index < haystack.length; index += 1) {
    const char = haystack[index];
    if (/\s/.test(char)) {
      if (!lastWasSpace) {
        normalized += " ";
        rawIndexes.push(index);
        lastWasSpace = true;
      }
      continue;
    }
    normalized += char;
    rawIndexes.push(index);
    lastWasSpace = false;
  }

  const matchAt = normalized.indexOf(wanted);
  if (matchAt < 0) return null;
  const rawIndex = rawIndexes[matchAt];
  if (rawIndex === undefined) return null;
  return locateRaw(pieces, rawIndex);
}

function caretRangeFromPoint(x: number, y: number): TextPosition | null {
  const doc = document as Document & {
    caretRangeFromPoint?: (x: number, y: number) => Range | null;
  };
  if (typeof doc.caretRangeFromPoint !== "function") return null;
  const range = doc.caretRangeFromPoint(x, y);
  if (!range) return null;
  const container = range.startContainer;
  if (container.nodeType !== 3) return null;
  return { node: container as Text, offset: range.startOffset };
}

function rectOf(node: Text): DOMRect | null {
  const range = document.createRange();
  range.selectNodeContents(node);
  const rect = range.getBoundingClientRect();
  return rect.width === 0 && rect.height === 0 ? null : rect;
}

/** 可视区里最靠前的可读文本位置 */
export function firstTextPositionIn(root: HTMLElement, viewport: DOMRect): TextPosition | null {
  const probeX = viewport.left + 6;
  const probeY = viewport.top + 6;
  const fromPoint = caretRangeFromPoint(probeX, probeY);
  if (fromPoint && root.contains(fromPoint.node)) {
    return fromPoint;
  }

  for (const node of collectTextNodes(root)) {
    const rect = rectOf(node);
    if (!rect) continue;
    if (
      rect.bottom > viewport.top &&
      rect.top < viewport.bottom &&
      rect.right > viewport.left &&
      rect.left < viewport.right
    ) {
      return { node, offset: 0 };
    }
  }
  return null;
}

export function captureAnchor(
  root: HTMLElement,
  viewport: DOMRect,
  ratio: number,
): TextAnchor | null {
  const position = firstTextPositionIn(root, viewport);
  if (!position) return null;
  const path = nodeToPath(root as unknown as NodeLike, position.node as unknown as NodeLike);
  if (!path) return null;
  return {
    path,
    offset: position.offset,
    snippet: snippetAround(position.node.data, position.offset),
    ratio: clampRatio(ratio),
  };
}

export function resolveAnchor(root: HTMLElement, anchor: TextAnchor): TextPosition | null {
  const node = pathToNode(root as unknown as NodeLike, anchor.path);
  if (node && (node as unknown as Node).nodeType === 3) {
    const text = node as unknown as Text;
    if (
      anchor.offset <= text.data.length &&
      snippetLooseMatch(text.data, anchor.offset, anchor.snippet)
    ) {
      return { node: text, offset: anchor.offset };
    }
    const local = findSnippetOffset(text.data, anchor.snippet);
    if (local >= 0) return { node: text, offset: local };
  }

  if (anchor.snippet) {
    const fallback = findTextPosition(root, anchor.snippet);
    if (fallback) return fallback;
  }
  return null;
}

/** 把文本位置转成一个 Range，供排版引擎测量坐标 */
export function rangeOf(position: TextPosition): Range {
  const range = document.createRange();
  range.setStart(position.node, Math.min(position.offset, position.node.data.length));
  range.collapse(true);
  return range;
}
