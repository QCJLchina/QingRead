/**
 * 分页排版的纯计算部分。
 *
 * 排版采用 CSS 多栏：把正文排成若干等宽栏，栏宽加栏间距正好等于「一页的步长」，
 * 于是第 n 栏的横坐标天然落在 n * stride，用 transform 平移 stride 就翻一页。
 * 这样列与页永远对齐，不会出现半页错位。
 */

export interface PagedMetricsInput {
  containerWidth: number;
  containerHeight: number;
  lineHeight: number;
  padding: number;
  /** 正文栏最大宽度；null / 0 表示跟随可用宽度 */
  maxContentWidth: number | null;
}

export interface PagedMetrics {
  /** 一页的水平步长 */
  stride: number;
  /** 单栏正文宽度 */
  columnWidth: number;
  /** 栏间距 = stride - columnWidth，保证下一栏正好落在下一页起点 */
  columnGap: number;
  /** 单栏可用高度，按整行取整，避免最后一行被裁掉 */
  height: number;
  /** 正文距离窗口边缘的留白 */
  padding: number;
  /** 正文相对窗口左边的偏移，用于把窄栏居中 */
  left: number;
}

const MIN_COLUMN_WIDTH = 120;

export function computePagedMetrics(input: PagedMetricsInput): PagedMetrics {
  const stride = Math.max(1, Math.floor(input.containerWidth));
  const padding = Math.max(0, Math.min(Math.floor(input.padding), Math.floor(stride / 4)));
  const lineHeight = Math.max(1, input.lineHeight || 1);

  const available = Math.max(MIN_COLUMN_WIDTH, stride - padding * 2);
  const requested = input.maxContentWidth && input.maxContentWidth > 0
    ? Math.floor(input.maxContentWidth)
    : available;
  const columnWidth = Math.max(MIN_COLUMN_WIDTH, Math.min(available, requested));
  const columnGap = Math.max(0, stride - columnWidth);

  const usableHeight = Math.max(lineHeight, input.containerHeight - padding * 2);
  const height = Math.max(lineHeight, Math.floor(usableHeight / lineHeight) * lineHeight);

  return {
    stride,
    columnWidth,
    columnGap,
    height,
    padding,
    left: (stride - columnWidth) / 2,
  };
}
