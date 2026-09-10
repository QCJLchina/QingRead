import { useEffect, useRef } from "react";
import type { RefObject } from "react";
import type { ReaderMode } from "../types";

export interface UseReaderNavigationOptions {
  mode: ReaderMode;
  /** 内容还没排好版时不要抢输入 */
  enabled: boolean;
  /** 浮层、遮挡页、书架视图打开时返回 true */
  blocked: () => boolean;
  containerRef: RefObject<HTMLDivElement>;
  move: (direction: -1 | 1) => void;
  onEscape: () => void;
}

/** 一次滚轮手势内只翻一页，用来过滤触控板的惯性滚动 */
const WHEEL_GESTURE_MS = 320;

function isEditableTarget(target: EventTarget | null): boolean {
  const element = target as HTMLElement | null;
  if (!element) return false;
  const tag = element.tagName;
  if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return true;
  return element.isContentEditable === true;
}

/**
 * 统一的翻页输入：方向键、PageUp/PageDown、空格、滚轮。
 *
 * 只负责把输入翻译成「上一页 / 下一页」，翻到章末要不要换章由调用方决定。
 */
export function useReaderNavigation(options: UseReaderNavigationOptions): void {
  const { mode, enabled, blocked, containerRef } = options;
  const lastWheelRef = useRef(0);
  const optionsRef = useRef(options);
  optionsRef.current = options;

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const current = optionsRef.current;
      if (event.key === "Escape") {
        current.onEscape();
        return;
      }
      if (!current.enabled || current.blocked() || isEditableTarget(event.target)) return;

      const forward =
        event.key === "ArrowRight" ||
        event.key === "ArrowDown" ||
        event.key === "PageDown" ||
        (event.key === " " && !event.shiftKey);
      const backward =
        event.key === "ArrowLeft" ||
        event.key === "ArrowUp" ||
        event.key === "PageUp" ||
        (event.key === " " && event.shiftKey);

      if (!forward && !backward) return;
      // 滚动模式下方向键和空格交给浏览器做自然滚动
      if (
        current.mode === "scroll" &&
        (event.key === "ArrowDown" || event.key === "ArrowUp" || event.key === " ")
      ) {
        return;
      }

      event.preventDefault();
      current.move(forward ? 1 : -1);
    };

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    if (mode !== "paged") return;

    const onWheel = (event: WheelEvent) => {
      const current = optionsRef.current;
      if (!current.enabled || current.blocked()) return;
      const target = event.target as HTMLElement | null;
      if (target && target.closest(".search-panel")) return;

      event.preventDefault();
      const delta = Math.abs(event.deltaX) > Math.abs(event.deltaY) ? event.deltaX : event.deltaY;
      if (Math.abs(delta) < 3) return;
      const now = Date.now();
      if (now - lastWheelRef.current < WHEEL_GESTURE_MS) return;
      lastWheelRef.current = now;
      current.move(delta > 0 ? 1 : -1);
    };

    container.addEventListener("wheel", onWheel, { passive: false });
    return () => container.removeEventListener("wheel", onWheel);
  }, [containerRef, mode]);
}
