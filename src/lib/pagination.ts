export function clampPage(page: number, pageCount: number): number {
  return Math.max(0, Math.min(Math.trunc(page), Math.max(1, pageCount) - 1));
}

export function pageFromRatio(ratio: number, pageCount: number): number {
  const safeRatio = Math.max(0, Math.min(1, ratio));
  return clampPage(Math.round(safeRatio * (Math.max(1, pageCount) - 1)), pageCount);
}

export function ratioFromPage(page: number, pageCount: number): number {
  if (pageCount <= 1) return 0;
  return clampPage(page, pageCount) / (pageCount - 1);
}

export function calculateBookProgress(
  chapterIndex: number,
  chapterCount: number,
  page: number,
  pageCount: number,
): number {
  if (chapterCount <= 0) return 0;
  const chapterProgress = (clampPage(page, pageCount) + 1) / Math.max(1, pageCount);
  return Math.min(100, Math.round(((chapterIndex + chapterProgress) / chapterCount) * 100));
}
