import { clampRatio } from "./anchor.ts";

export function clampPage(page: number, pageCount: number): number {
  return Math.max(0, Math.min(Math.trunc(page), Math.max(1, pageCount) - 1));
}

export function pageFromRatio(ratio: number, pageCount: number): number {
  const safeRatio = clampRatio(ratio);
  return clampPage(Math.round(safeRatio * (Math.max(1, pageCount) - 1)), pageCount);
}

export function ratioFromPage(page: number, pageCount: number): number {
  if (pageCount <= 1) return 0;
  return clampPage(page, pageCount) / (pageCount - 1);
}

/**
 * 一页的水平步长已知时，有多少页。
 * 分页排版按 stride 平移，所以页数就是「内容总宽 / 步长」向上取整。
 */
export function pageCountFromWidth(totalWidth: number, stride: number): number {
  if (!Number.isFinite(stride) || stride <= 0) return 1;
  if (!Number.isFinite(totalWidth) || totalWidth <= 0) return 1;
  return Math.max(1, Math.ceil(totalWidth / stride));
}

/** 某个横向坐标落在第几页 */
export function pageFromOffset(left: number, originLeft: number, stride: number): number {
  if (!Number.isFinite(stride) || stride <= 0) return 0;
  return Math.max(0, Math.floor((left - originLeft + 1) / stride));
}

/**
 * 全书进度：章节等权 + 章内比例。
 *
 * 滚动与分页共用同一个口径，避免两种模式下显示的百分比差一大截；
 * 也不再把章内页数直接加在章节序号上（那会让没读完的书显示接近 100%）。
 * 结果标注为「估算」，因为章节长度并不相等。
 */
export function estimateBookProgress(
  chapterIndex: number,
  chapterCount: number,
  ratioWithinChapter: number,
): number {
  if (chapterCount <= 0) return 0;
  const safeIndex = Math.max(0, Math.min(chapterIndex, chapterCount - 1));
  const ratio = clampRatio(ratioWithinChapter);
  return Math.min(100, Math.max(0, Math.round(((safeIndex + ratio) / chapterCount) * 100)));
}

/**
 * 兼容旧调用：按当前页在章内的位置估算全书进度。
 * 新代码应直接使用 estimateBookProgress。
 */
export function calculateBookProgress(
  chapterIndex: number,
  chapterCount: number,
  page: number,
  pageCount: number,
): number {
  if (chapterCount <= 0) return 0;
  const chapterProgress = (clampPage(page, pageCount) + 1) / Math.max(1, pageCount);
  return estimateBookProgress(chapterIndex, chapterCount, chapterProgress);
}
