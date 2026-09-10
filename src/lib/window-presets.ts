import type { WindowLayoutId } from "../types";

export interface WindowPreset {
  id: WindowLayoutId;
  label: string;
  width: number;
  height: number;
  description: string;
}

/** 客户区逻辑像素。与 tauri.conf.json 的 minWidth / minHeight 保持一致。 */
export const MIN_WINDOW_WIDTH = 320;
export const MIN_WINDOW_HEIGHT = 200;
export const MAX_WINDOW_WIDTH = 2400;
export const MAX_WINDOW_HEIGHT = 1600;

export const WINDOW_PRESETS: WindowPreset[] = [
  {
    id: "standard",
    label: "标准阅读",
    width: 940,
    height: 610,
    description: "完整工具栏，目录可以固定在左侧",
  },
  {
    id: "slim",
    label: "办公窄栏",
    width: 360,
    height: 610,
    description: "单栏正文，目录与设置改为浮层",
  },
  {
    id: "strip",
    label: "底部横条",
    width: 760,
    height: 270,
    description: "适合贴在屏幕底部，工具栏自动精简",
  },
  {
    id: "mini",
    label: "迷你阅读",
    width: 340,
    height: 230,
    description: "最小占用，保留翻页与退出",
  },
];

export function presetById(id: WindowLayoutId): WindowPreset | null {
  return WINDOW_PRESETS.find((preset) => preset.id === id) ?? null;
}

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}

/**
 * 把用户输入的宽高夹到允许范围，并限制在当前显示器可用区域内。
 * 上限只是「不要离谱」，真正的边界还是屏幕工作区。
 */
export function clampWindowSize(
  width: number,
  height: number,
  available?: { width: number; height: number },
): { width: number; height: number } {
  const maxWidth = available ? Math.min(MAX_WINDOW_WIDTH, available.width) : MAX_WINDOW_WIDTH;
  const maxHeight = available ? Math.min(MAX_WINDOW_HEIGHT, available.height) : MAX_WINDOW_HEIGHT;
  return {
    width: Math.round(clamp(width, MIN_WINDOW_WIDTH, Math.max(MIN_WINDOW_WIDTH, maxWidth))),
    height: Math.round(clamp(height, MIN_WINDOW_HEIGHT, Math.max(MIN_WINDOW_HEIGHT, maxHeight))),
  };
}

/** 锁定比例时，根据用户改动的那一边推算另一边 */
export function applyLockedRatio(
  width: number,
  height: number,
  ratio: number,
  changed: "width" | "height",
): { width: number; height: number } {
  if (!Number.isFinite(ratio) || ratio <= 0) {
    return { width, height };
  }
  if (changed === "width") {
    return { width, height: Math.round(width / ratio) };
  }
  return { width: Math.round(height * ratio), height };
}
