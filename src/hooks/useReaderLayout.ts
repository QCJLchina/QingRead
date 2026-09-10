import { useCallback, useEffect, useRef, useState } from "react";
import type { RefObject } from "react";
import { captureAnchor, rangeOf, resolveAnchor } from "../lib/anchor";
import { computePagedMetrics } from "../lib/layout";
import type { PagedMetrics } from "../lib/layout";
import { pageCountFromWidth, pageFromOffset, pageFromRatio } from "../lib/pagination";
import type { PendingPosition } from "../store/reader";
import type { ReaderMode, TextAnchor } from "../types";

export interface ReaderLayoutInfo {
  page: number;
  pageCount: number;
  ratio: number;
  anchor: TextAnchor | null;
}

export type MoveResult = "moved" | "next-chapter" | "prev-chapter" | "blocked";

export interface UseReaderLayoutOptions {
  containerRef: RefObject<HTMLDivElement>;
  trackRef: RefObject<HTMLDivElement>;
  mode: ReaderMode;
  /** 书籍 + 章节标识；变化时重新排版并应用新的待恢复位置 */
  contentKey: string;
  /** 章节内容已经渲染进 track */
  ready: boolean;
  fontSize: number;
  lineHeight: number;
  fontFamily: string;
  contentWidth: number | null;
  contentPadding: number;
  /** 章节加载时写入的待恢复位置 */
  pending: PendingPosition | null;
  onSettled: (info: ReaderLayoutInfo) => void;
}

const SETTLE_DEBOUNCE_MS = 60;
const SCROLL_TOP_INSET = 8;

function measureLineHeight(track: HTMLElement): number {
  const value = Number.parseFloat(window.getComputedStyle(track).lineHeight);
  return Number.isFinite(value) && value > 0 ? value : 28;
}

/** 最后一个非空文本字符的位置；元素节点递归下去找 */
function lastTextRect(node: Node): DOMRect | null {
  for (let index = node.childNodes.length - 1; index >= 0; index -= 1) {
    const child = node.childNodes[index];
    if (child.nodeType === 3) {
      const text = child as Text;
      if (text.data.trim().length === 0) continue;
      const range = document.createRange();
      const end = text.data.length;
      range.setStart(text, Math.max(0, end - 1));
      range.setEnd(text, end);
      const rect = range.getBoundingClientRect();
      if (rect.width > 0 || rect.height > 0) return rect;
      continue;
    }
    if (child.nodeType === 1) {
      const nested = lastTextRect(child);
      if (nested) return nested;
    }
  }
  return null;
}

/**
 * 页数取两种测量里的较大值：
 * - 多栏内容的总宽度除以步长（列盒模型下就是列数）；
 * - 最后一个字符的横坐标所在的列号加一。
 *
 * 两者正常情况下一致；万一某一种测量在特定 WebView 版本上不准，
 * 另一个仍然能给出正确页数，不至于出现「翻不到最后一页」。
 */
function measurePageCount(track: HTMLElement, stride: number): number {
  const fromWidth = pageCountFromWidth(track.scrollWidth, stride);
  const rect = lastTextRect(track);
  if (!rect) return fromWidth;
  const trackRect = track.getBoundingClientRect();
  const fromLastCharacter = pageFromOffset(rect.left, trackRect.left, stride) + 1;
  return Math.max(fromWidth, fromLastCharacter, 1);
}

async function waitForImages(root: HTMLElement): Promise<void> {
  const pending: Promise<void>[] = [];
  root.querySelectorAll("img").forEach((image) => {
    if (image.complete) return;
    pending.push(
      new Promise<void>((resolve) => {
        image.addEventListener("load", () => resolve(), { once: true });
        image.addEventListener("error", () => resolve(), { once: true });
      }),
    );
  });
  if (pending.length > 0) await Promise.all(pending);
}

function applyPagedStyle(track: HTMLElement, metrics: PagedMetrics): void {
  track.style.position = "absolute";
  track.style.top = metrics.padding + "px";
  track.style.left = metrics.left + "px";
  track.style.width = metrics.columnWidth + "px";
  track.style.height = metrics.height + "px";
  track.style.maxWidth = "none";
  track.style.margin = "0";
  track.style.padding = "0";
  track.style.columnWidth = metrics.columnWidth + "px";
  track.style.columnGap = metrics.columnGap + "px";
  track.style.columnFill = "auto";
  track.style.overflow = "visible";
}

function clearPagedStyle(track: HTMLElement): void {
  track.style.position = "";
  track.style.top = "";
  track.style.left = "";
  track.style.width = "";
  track.style.height = "";
  track.style.maxWidth = "";
  track.style.margin = "";
  track.style.padding = "";
  track.style.columnWidth = "";
  track.style.columnGap = "";
  track.style.columnFill = "";
  track.style.overflow = "";
  track.style.transform = "";
}

/**
 * 分页 / 滚动共用的排版引擎。
 *
 * 位置一律用文本锚点表示，页码只作为「当前排版下的显示结果」：
 * 调整窗口比例、字号、行距或开关目录时，先记下正在读的那句话，
 * 重新排版后再把这句话滚回可视区域。
 */
export function useReaderLayout(options: UseReaderLayoutOptions) {
  const {
    containerRef,
    trackRef,
    mode,
    contentKey,
    ready,
    fontSize,
    lineHeight,
    fontFamily,
    contentWidth,
    contentPadding,
    pending,
    onSettled,
  } = options;

  const [page, setPage] = useState(0);
  const [pageCount, setPageCount] = useState(1);
  const [settled, setSettled] = useState(false);

  const pageRef = useRef(0);
  const pageCountRef = useRef(1);
  const metricsRef = useRef<PagedMetrics | null>(null);
  const anchorRef = useRef<TextAnchor | null>(null);
  const pendingRef = useRef<PendingPosition | null>(null);
  /** 已经应用过的待恢复位置，避免 StrictMode 下重复应用同一个对象 */
  const appliedPendingRef = useRef<PendingPosition | null>(null);
  const layoutVersionRef = useRef(0);
  const capturedKeyRef = useRef<string | null>(null);
  const timerRef = useRef<number | null>(null);
  const settledRef = useRef(false);
  const onSettledRef = useRef(onSettled);
  onSettledRef.current = onSettled;

  useEffect(() => {
    if (!pending || appliedPendingRef.current === pending) return;
    pendingRef.current = pending;
  }, [pending]);

  /** 取出并消费待恢复位置；同一个对象不会再被应用第二次 */
  const takePending = useCallback((): PendingPosition | null => {
    const requested = pendingRef.current;
    pendingRef.current = null;
    if (requested) appliedPendingRef.current = requested;
    return requested;
  }, []);

  const currentRatio = useCallback((): number => {
    const container = containerRef.current;
    if (mode === "paged") {
      const count = pageCountRef.current;
      return count <= 1 ? 0 : pageRef.current / (count - 1);
    }
    if (!container) return 0;
    const max = Math.max(0, container.scrollHeight - container.clientHeight);
    return max <= 0 ? 0 : Math.min(1, Math.max(0, container.scrollTop / max));
  }, [containerRef, mode]);

  /** 记下当前正在读的那句话。重新排版前调用。 */
  const captureCurrentAnchor = useCallback(() => {
    const container = containerRef.current;
    const track = trackRef.current;
    if (!container || !track) return;

    const metrics = metricsRef.current;
    const containerRect = container.getBoundingClientRect();
    const trackRect = track.getBoundingClientRect();
    const viewport = metrics
      ? new DOMRect(trackRect.left, trackRect.top, metrics.columnWidth, metrics.height)
      : new DOMRect(
          containerRect.left + 4,
          containerRect.top + 4,
          Math.max(1, containerRect.width - 8),
          Math.max(1, containerRect.height - 8),
        );

    const captured = captureAnchor(track, viewport, currentRatio());
    if (captured) anchorRef.current = captured;
  }, [containerRef, currentRatio, trackRef]);

  const positionPage = useCallback(
    (target: number, stride: number) => {
      const track = trackRef.current;
      const safe = Math.max(0, Math.min(target, pageCountRef.current - 1));
      pageRef.current = safe;
      setPage(safe);
      if (track) {
        track.style.transform = "translateX(" + -safe * stride + "px)";
      }
    },
    [trackRef],
  );

  const report = useCallback(() => {
    onSettledRef.current({
      page: pageRef.current,
      pageCount: pageCountRef.current,
      ratio: currentRatio(),
      anchor: anchorRef.current,
    });
  }, [currentRatio]);

  const resolveTargetPage = useCallback(
    (count: number, metrics: PagedMetrics): number => {
      const track = trackRef.current;
      const requested = takePending();

      if (requested?.atEnd) return count - 1;

      const wanted = requested?.anchor ?? anchorRef.current;
      if (track && wanted) {
        const position = resolveAnchor(track, wanted);
        if (position) {
          const rect = rangeOf(position).getBoundingClientRect();
          const trackRect = track.getBoundingClientRect();
          return Math.max(
            0,
            Math.min(count - 1, pageFromOffset(rect.left, trackRect.left, metrics.stride)),
          );
        }
      }

      if (requested && typeof requested.ratio === "number") {
        return pageFromRatio(requested.ratio, count);
      }
      if (requested && typeof requested.page === "number") {
        return Math.max(0, Math.min(count - 1, requested.page));
      }
      return Math.max(0, Math.min(count - 1, pageRef.current));
    },
    [takePending, trackRef],
  );

  const restoreScroll = useCallback(() => {
    const container = containerRef.current;
    const track = trackRef.current;
    if (!container || !track) return;

    const requested = takePending();

    if (requested?.atEnd) {
      container.scrollTop = Math.max(0, container.scrollHeight - container.clientHeight);
      return;
    }

    const wanted = requested?.anchor ?? anchorRef.current;
    if (wanted) {
      const position = resolveAnchor(track, wanted);
      if (position) {
        const rect = rangeOf(position).getBoundingClientRect();
        const containerRect = container.getBoundingClientRect();
        container.scrollTop += rect.top - containerRect.top - SCROLL_TOP_INSET;
        return;
      }
    }

    const max = Math.max(0, container.scrollHeight - container.clientHeight);
    if (requested && typeof requested.ratio === "number") {
      container.scrollTop = requested.ratio * max;
      return;
    }
    if (requested && typeof requested.page === "number") {
      const step = Math.max(1, container.clientHeight - contentPadding);
      container.scrollTop = Math.min(max, requested.page * step);
    }
  }, [containerRef, contentPadding, takePending, trackRef]);

  const reflow = useCallback(async () => {
    const container = containerRef.current;
    const track = trackRef.current;
    if (!container || !track || !ready) return;

    const version = ++layoutVersionRef.current;
    const sameContent = capturedKeyRef.current === contentKey;
    // 换章时不要捕获上一章残留的内容，也不要把上一章的锚点套到新章节上
    if (sameContent) {
      if (settledRef.current) captureCurrentAnchor();
    } else {
      anchorRef.current = null;
    }
    setSettled(false);
    settledRef.current = false;

    if (mode === "paged") {
      track.style.transform = "translateX(0px)";
      const metrics = computePagedMetrics({
        containerWidth: container.clientWidth,
        containerHeight: container.clientHeight,
        lineHeight: measureLineHeight(track),
        padding: contentPadding,
        maxContentWidth: contentWidth,
      });
      applyPagedStyle(track, metrics);
      metricsRef.current = metrics;

      await waitForImages(track);
      if (version !== layoutVersionRef.current) return;

      const count = measurePageCount(track, metrics.stride);
      const target = resolveTargetPage(count, metrics);
      pageCountRef.current = count;
      setPageCount(count);
      positionPage(target, metrics.stride);
      capturedKeyRef.current = contentKey;
      setSettled(true);
      settledRef.current = true;
      report();
      return;
    }

    metricsRef.current = null;
    clearPagedStyle(track);
    await waitForImages(track);
    if (version !== layoutVersionRef.current) return;

    restoreScroll();
    pageCountRef.current = 1;
    pageRef.current = 0;
    setPageCount(1);
    setPage(0);
    capturedKeyRef.current = contentKey;
    setSettled(true);
    settledRef.current = true;
    report();
  }, [
    captureCurrentAnchor,
    containerRef,
    contentKey,
    contentPadding,
    contentWidth,
    mode,
    positionPage,
    ready,
    report,
    resolveTargetPage,
    restoreScroll,
    trackRef,
  ]);

  useEffect(() => {
    if (!ready) return;
    if (timerRef.current !== null) window.clearTimeout(timerRef.current);
    timerRef.current = window.setTimeout(() => {
      timerRef.current = null;
      void reflow();
    }, SETTLE_DEBOUNCE_MS);
    return () => {
      if (timerRef.current !== null) {
        window.clearTimeout(timerRef.current);
        timerRef.current = null;
      }
    };
  }, [reflow, ready, contentKey, mode, fontSize, lineHeight, fontFamily, contentWidth, contentPadding]);

  useEffect(() => {
    const container = containerRef.current;
    if (!container || !ready) return;
    const observer = new ResizeObserver(() => {
      void reflow();
    });
    observer.observe(container);
    return () => observer.disconnect();
  }, [containerRef, ready, reflow]);

  useEffect(
    () => () => {
      layoutVersionRef.current += 1;
    },
    [],
  );

  const goPage = useCallback(
    (delta: number): MoveResult => {
      const container = containerRef.current;
      if (mode === "paged") {
        const next = pageRef.current + delta;
        const metrics = metricsRef.current;
        if (next >= 0 && next < pageCountRef.current && metrics) {
          positionPage(next, metrics.stride);
          report();
          return "moved";
        }
        return delta > 0 ? "next-chapter" : "prev-chapter";
      }

      if (!container) return "blocked";
      const max = Math.max(0, container.scrollHeight - container.clientHeight);
      const step = Math.max(24, container.clientHeight - lineHeight * 1.5);
      if (delta > 0 && container.scrollTop < max - 1) {
        container.scrollTop = Math.min(max, container.scrollTop + step);
        report();
        return "moved";
      }
      if (delta < 0 && container.scrollTop > 1) {
        container.scrollTop = Math.max(0, container.scrollTop - step);
        report();
        return "moved";
      }
      return delta > 0 ? "next-chapter" : "prev-chapter";
    },
    [containerRef, lineHeight, mode, positionPage, report],
  );

  const snapshot = useCallback((): ReaderLayoutInfo => {
    captureCurrentAnchor();
    return {
      page: pageRef.current,
      pageCount: pageCountRef.current,
      ratio: currentRatio(),
      anchor: anchorRef.current,
    };
  }, [captureCurrentAnchor, currentRatio]);

  /** 滚动模式下持续更新锚点与进度（节流，避免每帧写盘） */
  useEffect(() => {
    if (mode !== "scroll" || !ready) return;
    const container = containerRef.current;
    if (!container) return;
    let lastReported = 0;
    const onScroll = () => {
      captureCurrentAnchor();
      const now = Date.now();
      if (now - lastReported < 250) return;
      lastReported = now;
      report();
    };
    container.addEventListener("scroll", onScroll, { passive: true });
    return () => container.removeEventListener("scroll", onScroll);
  }, [captureCurrentAnchor, containerRef, mode, ready, report]);

  return {
    page,
    pageCount,
    settled,
    reflow,
    goPage,
    snapshot,
    captureCurrentAnchor,
  };
}
